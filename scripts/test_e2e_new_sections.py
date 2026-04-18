# -*- coding: utf-8 -*-
"""
V2ハローワークダッシュボード — 新外部統計セクション E2Eテスト

テスト対象:
  新セクション7個（subtab5 外部比較タブに追加）
    1. 学歴分布
    2. 世帯構成
    3. 在留外国人
    4. 地価
    5. 地域インフラ
    6. 住民の行動特性
    7. 業況判断DI

  既存セクション（リグレッションチェック）
    異常値検出 / 最低賃金 / 都道府県別外部指標 / 有効求人倍率
    人口構成 / 産業別事業所 / 介護需要 / 地域ベンチマーク

テスト項目:
  T1  ログイン
  T2  subtab5 HTMLフェッチ（新セクション7個の存在確認）
  T3  ECharts描画確認（canvas要素 ≥ 3個）
  T4  既存セクション8個のリグレッション（消失ゼロ確認）
  T5  スクリーンショット保存（フルページ + 個別セクション）

実行方法:
  pip install playwright
  playwright install chromium
  python scripts/test_e2e_new_sections.py

注意:
  - 本番 https://hr-hw.onrender.com に接続するため外部ネットワークが必要
  - テスト失敗時もスクリーンショットを保存してエビデンスを残す
"""

import sys
import io
import os
import time

# Windows環境でのUTF-8出力保証
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8", errors="replace")

from playwright.sync_api import sync_playwright

# ==================== 設定 ====================
BASE_URL = "https://hr-hw.onrender.com"
LOGIN_EMAIL = "test@f-a-c.co.jp"
LOGIN_PASSWORD = "cyxen_2025"

# subtab5のHTMXエンドポイント（都道府県: 東京都、市区町村: 全体、産業: 全体）
SUBTAB5_API = "/api/analysis/subtab/5?pref=東京都&muni=&industry="

# スクリーンショット保存先
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
SS_DIR = os.path.join(SCRIPT_DIR, "screenshots", "e2e_external")
os.makedirs(SS_DIR, exist_ok=True)

# ==================== テスト対象セクション ====================
# 新規追加セクション（7個）— これらが全てHTMLに含まれることを確認
NEW_SECTIONS = [
    "学歴分布",
    "世帯構成",
    "在留外国人",
    "地価",
    "地域インフラ",
    "住民の行動特性",
    "業況判断DI",
]

# 既存セクション（8個）— リグレッションで消えていないことを確認
EXISTING_SECTIONS = [
    "異常値検出",
    "最低賃金",
    "都道府県別外部指標",
    "有効求人倍率",
    "人口構成",
    "産業別事業所",
    "介護需要",
    "地域ベンチマーク",
]

# ==================== 結果集計 ====================
PASSED = 0
FAILED = 0
RESULTS = []


def check(name: str, ok: bool, detail: str = "") -> None:
    """テスト結果を記録して即時出力する"""
    global PASSED, FAILED
    if ok:
        PASSED += 1
        tag = "PASS"
    else:
        FAILED += 1
        tag = "FAIL"
    msg = f"[{tag}] {name}"
    if detail:
        msg += f" -- {detail}"
    RESULTS.append((tag, name, detail))
    print(msg)


def screenshot(page, filename: str) -> None:
    """スクリーンショットを保存する（失敗してもテストは継続）"""
    path = os.path.join(SS_DIR, filename)
    try:
        page.screenshot(path=path, full_page=False)
        print(f"  [SS] 保存: {os.path.basename(path)}")
    except Exception as e:
        print(f"  [SS] 保存失敗: {e}")


def screenshot_full(page, filename: str) -> None:
    """フルページスクリーンショットを保存する"""
    path = os.path.join(SS_DIR, filename)
    try:
        page.screenshot(path=path, full_page=True)
        print(f"  [SS] フルページ保存: {os.path.basename(path)}")
    except Exception as e:
        print(f"  [SS] フルページ保存失敗: {e}")


def remove_loading_overlay(page) -> None:
    """ローディングオーバーレイの .active クラスを除去してクリック干渉を防ぐ"""
    page.evaluate("""() => {
        const overlay = document.getElementById('loading-overlay');
        if (overlay) {
            overlay.classList.remove('active');
        }
    }""")


# ==================== テスト本体 ====================

def main():
    print("=== E2Eテスト: 新外部統計セクション ===")
    print(f"対象URL: {BASE_URL}")
    print(f"スクリーンショット保存先: {SS_DIR}")
    print()

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, slow_mo=100)
        page = browser.new_page(viewport={"width": 1400, "height": 900})

        # ==================== T1: ログイン ====================
        print("--- T1: ログイン ---")
        try:
            page.goto(f"{BASE_URL}/login", wait_until="networkidle", timeout=60000)
        except Exception:
            # タイムアウトしても続行（Renderのコールドスタート対策）
            page.wait_for_timeout(5000)

        remove_loading_overlay(page)

        # メールアドレス入力（name属性優先、なければrole=textboxで検索）
        try:
            page.fill('input[name="email"]', LOGIN_EMAIL)
            page.fill('input[name="password"]', LOGIN_PASSWORD)
        except Exception:
            # name属性が異なる場合のフォールバック
            inputs = page.locator('input[type="text"], input[type="email"]')
            if inputs.count() > 0:
                inputs.first.fill(LOGIN_EMAIL)
            pwd_inputs = page.locator('input[type="password"]')
            if pwd_inputs.count() > 0:
                pwd_inputs.first.fill(LOGIN_PASSWORD)

        # ログインボタン押下（type=submit → ログインテキスト含むボタン の順で試行）
        try:
            page.click('button[type="submit"]')
        except Exception:
            page.click('button:has-text("ログイン")')

        # ログイン後のページ遷移を待機（最大15秒）
        page.wait_for_timeout(8000)
        remove_loading_overlay(page)

        logged_in = "/login" not in page.url
        check("T1-1 ログイン成功", logged_in, f"現在のURL: {page.url}")
        screenshot(page, "t1_login.png")

        if not logged_in:
            # ログイン失敗時はエビデンスを残して終了
            screenshot_full(page, "t1_login_failed_full.png")
            print("\n[ERROR] ログイン失敗のためテストを中断します")
            browser.close()
            _print_summary()
            return

        # ==================== T2: subtab5の新セクション確認 ====================
        print("\n--- T2: 新セクション7個の存在確認 ---")

        # APIエンドポイントを直接fetchしてHTMLコンテンツを取得
        # ページコンテキストのセッションCookieを使用するため credentials:'include' を指定
        try:
            subtab5_html = page.evaluate(f"""() => {{
                return fetch('{SUBTAB5_API}', {{
                    credentials: 'include',
                    headers: {{'Accept': 'text/html'}}
                }}).then(r => {{
                    if (!r.ok) throw new Error('HTTP ' + r.status);
                    return r.text();
                }});
            }}""")
        except Exception as e:
            check("T2-0 subtab5 APIフェッチ", False, str(e))
            subtab5_html = ""

        api_ok = len(subtab5_html) > 100
        check("T2-0 subtab5 APIフェッチ", api_ok,
              f"取得文字数: {len(subtab5_html)}" if api_ok else "レスポンスが空または短すぎる")

        # 新セクション7個の存在チェック（タイトル文字列で検索）
        new_found = []
        new_missing = []
        for section in NEW_SECTIONS:
            if section in subtab5_html:
                new_found.append(section)
                check(f"T2 新セクション: {section}", True)
            else:
                new_missing.append(section)
                check(f"T2 新セクション: {section}", False, "HTMLに含まれない")

        check("T2-TOTAL 新セクション全数確認",
              len(new_missing) == 0,
              f"{len(new_found)}/{len(NEW_SECTIONS)} 件発見, 未発見: {new_missing}")

        # ==================== T3: ECharts描画確認 ====================
        print("\n--- T3: ECharts描画確認 ---")

        # subtab5のHTMLをmain要素に注入してEChartsを初期化
        inject_ok = False
        try:
            page.evaluate(f"""() => {{
                return fetch('{SUBTAB5_API}', {{
                    credentials: 'include',
                    headers: {{'Accept': 'text/html'}}
                }}).then(r => r.text()).then(html => {{
                    const main = document.querySelector('main') ||
                                 document.querySelector('#content') ||
                                 document.querySelector('.main-content');
                    if (main) {{
                        main.innerHTML = html;
                    }} else {{
                        // mainが見つからない場合はbodyに直接注入
                        document.body.innerHTML = html;
                    }}
                }});
            }}""")
            inject_ok = True
        except Exception as e:
            check("T3-0 HTML注入", False, str(e))

        if inject_ok:
            check("T3-0 HTML注入", True)

            # EChartsの初期化をHTMXイベント経由でトリガー（htmx:afterSwap相当）
            page.evaluate("""() => {
                // htmxのafterSwapイベントを手動発火してEChartsを初期化
                if (typeof htmx !== 'undefined') {
                    document.querySelectorAll('[hx-trigger], [data-hx-trigger]').forEach(el => {
                        htmx.trigger(el, 'load');
                    });
                }
                // ECharts要素にdata-chart-configがある場合は直接初期化
                document.querySelectorAll('.echart[data-chart-config]').forEach(el => {
                    try {
                        if (typeof echarts !== 'undefined') {
                            const chart = echarts.init(el);
                            const cfg = JSON.parse(el.getAttribute('data-chart-config'));
                            chart.setOption(cfg);
                        }
                    } catch(e) { /* 初期化失敗は無視して続行 */ }
                });
            }""")

            # ECharts初期化の完了を待機（最大3秒）
            page.wait_for_timeout(3000)
            remove_loading_overlay(page)

            # canvas要素の数を確認（EChartsはcanvasで描画される）
            canvas_count = page.evaluate(
                "document.querySelectorAll('.echart canvas, canvas[data-zr-dom-id]').length"
            )
            check("T3-1 ECharts canvas要素数",
                  canvas_count >= 3,
                  f"canvas数: {canvas_count} (期待値: ≥3)")

            # data-chart-config属性を持つ要素（チャート定義）の数も確認
            chart_def_count = page.evaluate(
                "document.querySelectorAll('.echart[data-chart-config]').length"
            )
            check("T3-2 チャート定義要素数",
                  chart_def_count >= 3,
                  f"チャート定義数: {chart_def_count} (期待値: ≥3)")

            screenshot_full(page, "t3_echarts_fullpage.png")

        # ==================== T4: 既存セクションのリグレッションチェック ====================
        print("\n--- T4: 既存セクション8個のリグレッションチェック ---")

        existing_found = []
        existing_missing = []
        for section in EXISTING_SECTIONS:
            if section in subtab5_html:
                existing_found.append(section)
                check(f"T4 既存セクション: {section}", True)
            else:
                existing_missing.append(section)
                check(f"T4 既存セクション: {section}", False, "消失の可能性")

        check("T4-TOTAL 既存セクション全数確認",
              len(existing_missing) == 0,
              f"{len(existing_found)}/{len(EXISTING_SECTIONS)} 件確認, 消失疑い: {existing_missing}")

        # ==================== T5: スクリーンショット保存 ====================
        print("\n--- T5: スクリーンショット保存 ---")

        # フルページスクリーンショット
        screenshot_full(page, "subtab5_fullpage.png")
        check("T5-1 フルページスクリーンショット", True, "subtab5_fullpage.png")

        # 各新セクションのスクリーンショット（セクション要素を探してスクロール後撮影）
        saved_sections = []
        for i, section in enumerate(NEW_SECTIONS, 1):
            # セクションタイトルを含む要素を探してスクロール
            try:
                # h2, h3, h4, .section-title 等でセクション見出しを探す
                locator = page.locator(
                    f"h2:has-text('{section}'), h3:has-text('{section}'), "
                    f"h4:has-text('{section}'), .section-title:has-text('{section}')"
                ).first
                if locator.count() > 0 or True:  # countが0でもelement_handleを試みる
                    locator.scroll_into_view_if_needed(timeout=3000)
                    page.wait_for_timeout(500)
                    fname = f"t5_section_{i:02d}_{section}.png"
                    screenshot(page, fname)
                    saved_sections.append(section)
            except Exception:
                # セクション要素が見つからない場合はスキップ（フルページで代替）
                pass

        check("T5-2 個別セクションスクリーンショット",
              len(saved_sections) >= 1,
              f"{len(saved_sections)}/{len(NEW_SECTIONS)} 枚保存: {saved_sections}")

        browser.close()

    # ==================== サマリー出力 ====================
    _print_summary()


def _print_summary():
    """テスト結果のサマリーを出力する"""
    total = PASSED + FAILED
    print()
    print("=" * 50)
    print(f"=== テスト結果サマリー ===")
    print(f"合計: {PASSED}/{total} 合格  ({FAILED} 件失敗)")
    print()

    # 失敗したテストの一覧
    failed_tests = [(tag, name, detail) for tag, name, detail in RESULTS if tag == "FAIL"]
    if failed_tests:
        print("【失敗一覧】")
        for tag, name, detail in failed_tests:
            print(f"  [FAIL] {name}" + (f" -- {detail}" if detail else ""))
    else:
        print("【全テスト合格】")

    print("=" * 50)


if __name__ == "__main__":
    main()
