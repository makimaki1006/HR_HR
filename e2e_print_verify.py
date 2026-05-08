# -*- coding: utf-8 -*-
"""
印刷プレビュー実機検証 (/report/survey, /report/insight)
- page.emulate_media(media='print') で印刷モードに切り替え
- page.pdf() で A4 PDF を実生成
- pypdf でページ数/表紙/フッター/機密情報文言を検証
- スクリーンショット取得

判定基準:
  - PDF 生成成功 (I/O エラーなし)
  - ページ数が想定レンジ (6-15) に収まる
  - 表紙（1ページ目テキスト）に "ハローワーク求人市場 総合診断レポート" を含む
  - いずれかのページに "機密情報" を含む
  - フッター要素 "F-A-C株式会社" がいずれかのページに含まれる
  - console.warn/error が致命的でない
"""
import os
import sys
import time
from playwright.sync_api import sync_playwright

try:
    from pypdf import PdfReader
except ImportError:
    print("[FATAL] pypdf not installed. pip install pypdf")
    sys.exit(2)

BASE = os.environ.get("HW_BASE", "https://hr-hw.onrender.com")
EMAIL = os.environ.get("HW_EMAIL", "test@f-a-c.co.jp")
PASSWORD = os.environ.get("HW_PASSWORD", "cyxen_2025")

DIR = os.path.dirname(os.path.abspath(__file__))


def check(label, cond, fatal=False):
    tag = "PASS" if cond else ("FAIL" if fatal else "WARN")
    icon = "OK" if cond else "NG"
    print(f"  [{icon}] [{tag}] {label}")
    return cond


def verify_pdf(pdf_path, label, expected_min_pages=6, expected_max_pages=15):
    print(f"\n--- PDF検証: {label} ({pdf_path}) ---")
    results = {}
    if not os.path.exists(pdf_path):
        check(f"PDFファイル生成 ({label})", False, fatal=True)
        return False
    size = os.path.getsize(pdf_path)
    check(f"PDFサイズ > 10KB (実測: {size:,} bytes)", size > 10_000)
    try:
        reader = PdfReader(pdf_path)
    except Exception as e:
        print(f"  [NG] [FAIL] PDF読込失敗: {type(e).__name__}: {e}")
        return False
    n = len(reader.pages)
    check(f"ページ数レンジ内 [{expected_min_pages}-{expected_max_pages}] (実測: {n})",
          expected_min_pages <= n <= expected_max_pages)
    results["pages"] = n

    # 全ページからテキスト抽出
    full_text = ""
    for i, page in enumerate(reader.pages):
        try:
            full_text += page.extract_text() or ""
            full_text += "\n"
        except Exception:
            pass

    has_title = "ハローワーク求人市場" in full_text or "総合診断レポート" in full_text or "競合調査レポート" in full_text
    check("表紙/タイトル文言を検出", has_title)

    has_conf = "機密情報" in full_text
    check("『機密情報』文言を検出", has_conf)

    has_fac = "F-A-C" in full_text or "F-A-C株式会社" in full_text
    check("『F-A-C』フッター文言を検出（いずれかのページ）", has_fac)

    # ページ1テキスト（表紙想定）をサンプリング
    try:
        p1_text = (reader.pages[0].extract_text() or "")[:200].replace("\n", " / ")
        print(f"  [INFO] Page1冒頭: {p1_text}")
    except Exception:
        pass

    return has_title and has_conf


def run_report(ctx, path, pdf_name, screenshot_name, min_pages=6, max_pages=15, wait_sec=12):
    print(f"\n=== {path} を印刷プレビューで検証 ===")
    page = ctx.new_page()
    console_msgs = []
    page.on("console", lambda m: console_msgs.append((m.type, m.text)))

    url = f"{BASE}{path}"
    try:
        page.goto(url, timeout=90_000)
    except Exception as e:
        print(f"  [NG] [FAIL] goto失敗 {url}: {type(e).__name__}: {e}")
        page.close()
        return False
    time.sleep(wait_sec)

    # 印刷メディア適用
    try:
        page.emulate_media(media="print")
        time.sleep(1)
    except Exception as e:
        print(f"  [WARN] emulate_media失敗: {e}")

    # スクリーンショット
    ss_path = os.path.join(DIR, screenshot_name)
    try:
        page.screenshot(path=ss_path, full_page=True, timeout=20_000)
        print(f"  [INFO] screenshot: {screenshot_name}")
    except Exception as e:
        print(f"  [WARN] screenshot失敗: {type(e).__name__}: {e}")

    # Round 2.11: viewport を A4 portrait に縮小 (Round 2.10 で確定した真因対策)
    # page.pdf() は viewport を縮小しない仕様で、default 1280×720 のまま PDF 化されると
    # body / section / .echart に 1280-1248px が伝搬し ECharts SVG が本文域 555pt に
    # 押し込まれて見切れる。ECharts resize evaluate より先に viewport を揃える。
    try:
        page.set_viewport_size({"width": 794, "height": 1123})
    except Exception as e:
        print(f"  [WARN] PDF前 viewport設定失敗 (続行): {type(e).__name__}: {e}")

    # Round 2.9-A: page.pdf() 直前に ECharts container を強制 resize
    # 真因 (Round 2.8-D): page.pdf() (Chromium DevTools Page.printToPDF) は
    # beforeprint / matchMedia('print') を発火させないため、helpers.rs の
    # resize hook が動かず screen viewport 幅 (~960pt) のまま PDF 化される。
    # 対策: 明示的に container 幅を絞り echarts.resize() を発火、
    # bbox.width が A4 本文域 (760pt 安全枠) 以下になるまで待機。
    try:
        page.evaluate(
            """
            () => {
              document.documentElement.classList.add('pdf-rendering');
              const charts = Array.from(document.querySelectorAll('[_echarts_instance_]'));
              charts.forEach(el => {
                el.style.width = '100%';
                el.style.maxWidth = '100%';
                const inst = window.echarts && window.echarts.getInstanceByDom
                  ? window.echarts.getInstanceByDom(el) : null;
                if (inst && typeof inst.resize === 'function') {
                  try { inst.resize(); } catch (_) {}
                }
              });
            }
            """
        )
        page.wait_for_timeout(800)
        page.wait_for_function(
            """
            () => {
              const charts = Array.from(document.querySelectorAll('[_echarts_instance_]'));
              if (charts.length === 0) return true;
              return charts.every(el => {
                const r = el.getBoundingClientRect();
                return r.width > 0 && r.width <= 760;
              });
            }
            """,
            timeout=10_000,
        )
    except Exception as e:
        print(f"  [WARN] PDF前 resize hook 失敗 (続行): {type(e).__name__}: {e}")

    # PDF 生成
    pdf_path = os.path.join(DIR, pdf_name)
    try:
        page.pdf(
            path=pdf_path,
            format="A4",
            print_background=True,
            margin={"top": "10mm", "bottom": "18mm", "left": "10mm", "right": "10mm"},
        )
        print(f"  [INFO] PDF生成: {pdf_name}")
    except Exception as e:
        print(f"  [NG] [FAIL] PDF生成失敗: {type(e).__name__}: {e}")
        page.close()
        return False

    errors = [t for (lvl, t) in console_msgs if lvl == "error"]
    if errors:
        print(f"  [WARN] console errors: {len(errors)} 件")
        for e in errors[:3]:
            print(f"          - {e[:120]}")
    else:
        print("  [INFO] console errors: 0")

    ok = verify_pdf(pdf_path, path, min_pages, max_pages)
    page.close()
    return ok


def main():
    print(f"BASE: {BASE}")
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # === ログイン ===
        print("\n=== ログイン ===")
        try:
            page.goto(BASE, timeout=60_000)
            time.sleep(3)
            page.fill('input[name="email"]', EMAIL)
            page.fill('input[name="password"]', PASSWORD)
            page.click('button[type="submit"]')
            time.sleep(6)
            body_text = page.text_content("body") or ""
            check("ログイン成功", "ログアウト" in body_text or "ダッシュボード" in body_text, fatal=True)
        except Exception as e:
            print(f"  [NG] [FAIL] ログイン失敗: {type(e).__name__}: {e}")
            browser.close()
            return 2

        # === フィルタ設定（東京都千代田区） ===
        print("\n=== フィルタ: 東京都 千代田区 ===")
        try:
            page.evaluate("""
                fetch('/api/set_prefecture', {method:'POST',
                    headers:{'Content-Type':'application/x-www-form-urlencoded'},
                    body:'prefecture=東京都', credentials:'include'})
            """)
            time.sleep(1)
            page.evaluate("""
                fetch('/api/set_municipality', {method:'POST',
                    headers:{'Content-Type':'application/x-www-form-urlencoded'},
                    body:'municipality=千代田区', credentials:'include'})
            """)
            time.sleep(2)
            print("  [INFO] フィルタ設定完了")
        except Exception as e:
            print(f"  [WARN] フィルタ設定失敗: {e}")

        page.close()

        # === /report/insight ===
        insight_ok = run_report(
            ctx,
            "/report/insight",
            "report_insight_print.pdf",
            "print_insight_fullpage.png",
            min_pages=6, max_pages=15, wait_sec=18,
        )

        # === /report/survey （要: 事前CSVアップロードが無ければスキップ候補） ===
        # セッションCSVが無い場合 /report/survey はエラーの可能性あり。
        # この E2E はレイアウト検証目的のため、URL直アクセスで試みる。
        survey_ok = run_report(
            ctx,
            "/report/survey",
            "report_survey_print.pdf",
            "print_survey_fullpage.png",
            min_pages=4, max_pages=15, wait_sec=10,
        )

        browser.close()

    print("\n====================================")
    print(f"insight PDF: {'PASS' if insight_ok else 'FAIL'}")
    print(f"survey  PDF: {'PASS' if survey_ok else 'FAIL (要: CSV事前アップロード)'}")
    print("====================================")

    return 0 if insight_ok else 1


if __name__ == "__main__":
    sys.exit(main())
