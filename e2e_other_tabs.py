# -*- coding: utf-8 -*-
"""
主要5タブ E2E検証
- 市場概況 / 地図 / 詳細分析 / 企業検索 / 条件診断
- 既存E2Eパターン(e2e_report_survey, e2e_report_insight)踏襲
- 要素存在だけでなく「データ妥当性」まで検証
  (MEMORY: feedback_test_data_validation / feedback_e2e_chart_verification)
"""
import os, time, re
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
EMAIL = "test@f-a-c.co.jp"
PASSWORD = "cyxen_2025"
DIR = os.path.dirname(os.path.abspath(__file__))

# グローバル集計
TOTAL = 0
PASSED = 0
FAILED = 0


def ss(page, name, full=False):
    """スクリーンショット保存 (失敗しても継続)"""
    time.sleep(1)
    path = os.path.join(DIR, f"other_{name}.png")
    try:
        page.screenshot(path=path, full_page=full, timeout=15000)
        print(f"    [screenshot] other_{name}.png")
    except Exception as e:
        print(f"    [screenshot-FAIL] other_{name}.png: {type(e).__name__}")


def check(label, cond):
    """PASS/FAIL判定 + グローバル集計"""
    global TOTAL, PASSED, FAILED
    TOTAL += 1
    if cond:
        PASSED += 1
    else:
        FAILED += 1
    status = "PASS" if cond else "FAIL"
    icon = "OK" if cond else "NG"
    print(f"  [{icon}] [{status}] {label}")
    return cond


def info(msg):
    print(f"  [INFO] {msg}")


def click_tab(page, keyword):
    """タブボタンをテキストマッチでクリック (実クリック)"""
    for btn in page.query_selector_all('.tab-btn'):
        text = (btn.text_content() or "").strip()
        if keyword in text:
            btn.click()
            return True
    return False


def wait_content_loaded(page, min_len=500, timeout_sec=20):
    """#content が min_len 以上の長さになるまで待機"""
    for i in range(timeout_sec):
        text_len = page.evaluate(
            "(function(){var c=document.getElementById('content');"
            "return c ? (c.textContent||'').length : 0;})()"
        )
        if text_len >= min_len:
            return text_len, i + 1
        time.sleep(1)
    return text_len, timeout_sec


def nan_check(page):
    """NaN/undefined 文字列が content に含まれないことを確認"""
    bad = page.evaluate("""
        (function(){
            var c = document.getElementById('content');
            if (!c) return {found: false, reasons: []};
            var text = c.textContent || '';
            var reasons = [];
            if (text.indexOf('NaN') >= 0) reasons.push('NaN');
            if (text.indexOf('undefined') >= 0) reasons.push('undefined');
            if (text.indexOf('null,null') >= 0) reasons.push('null,null');
            return {found: reasons.length > 0, reasons: reasons};
        })()
    """)
    return bad


# =========================================================
# 各タブテスト
# =========================================================

def test_market(page):
    """1. 市場概況タブ"""
    print("\n=== [市場概況タブ] ===")
    clicked = click_tab(page, "市場")
    if not clicked:
        check("市場概況タブボタン検出", False)
        return
    text_len, sec = wait_content_loaded(page, min_len=500)
    info(f"タブ読み込み {text_len}文字 ({sec}秒)")
    check("タブ読み込み完了 (>500文字)", text_len > 500)
    ss(page, "01_market")

    body = page.text_content("#content") or ""

    # KPI: 総求人数抽出
    m = re.search(r'総求人数[^\d]*([\d,]+)', body)
    if m:
        total_jobs = int(m.group(1).replace(",", ""))
        info(f"総求人数: {total_jobs:,}")
        check("総求人数が妥当範囲 (400K-500K)", 400_000 <= total_jobs <= 500_000)
    else:
        check("総求人数KPI抽出", False)

    # 欠員率
    m2 = re.search(r'欠員率[^\d]*([\d.]+)\s*%', body)
    if m2:
        rate = float(m2.group(1))
        info(f"欠員率: {rate}%")
        check("欠員率が0-100範囲", 0.0 <= rate <= 100.0)
    else:
        info("欠員率KPI未検出 (スキップ)")

    # サブタブ存在確認
    for sub in ["概況", "雇用条件", "企業分析", "採用動向"]:
        check(f"サブタブ「{sub}」存在", sub in body)

    # ECharts
    chart_count = page.evaluate(
        "document.querySelectorAll('[data-chart-config]').length"
    )
    info(f"data-chart-config数: {chart_count}")
    check("EChartsチャート >=2", chart_count >= 2)

    # チャート初期化
    initialized = page.evaluate("""
        (function(){
            if (typeof echarts === 'undefined') return 0;
            var n = 0;
            document.querySelectorAll('[data-chart-config]').forEach(function(el){
                if (echarts.getInstanceByDom(el)) n++;
            });
            return n;
        })()
    """)
    info(f"ECharts初期化済み: {initialized}")
    check("EChartsインスタンス >=2", initialized >= 2)

    # データ点数 (棒グラフ)
    data_points = page.evaluate("""
        (function(){
            var max = 0;
            document.querySelectorAll('[data-chart-config]').forEach(function(el){
                try {
                    var cfg = JSON.parse(el.getAttribute('data-chart-config'));
                    if (cfg.series && cfg.series[0] && cfg.series[0].data) {
                        if (cfg.series[0].data.length > max) max = cfg.series[0].data.length;
                    }
                } catch(e){}
            });
            return max;
        })()
    """)
    info(f"最大データ点数: {data_points}")
    check("棒グラフのデータ点 >5", data_points > 5)

    bad = nan_check(page)
    check(f"NaN/undefined なし (detected: {bad.get('reasons')})", not bad.get('found'))


def test_jobmap(page):
    """2. 地図タブ"""
    print("\n=== [地図タブ] ===")
    clicked = click_tab(page, "地図")
    if not clicked:
        check("地図タブボタン検出", False)
        return
    text_len, sec = wait_content_loaded(page, min_len=500)
    info(f"タブ読み込み {text_len}文字 ({sec}秒)")
    check("タブ読み込み完了 (>500文字)", text_len > 500)
    time.sleep(3)  # Leaflet初期化待ち
    ss(page, "02_jobmap")

    # Leafletマップ
    leaflet = page.evaluate(
        "document.querySelectorAll('.leaflet-container').length"
    )
    info(f"Leafletコンテナ数: {leaflet}")
    check("Leafletマップ存在 (>=1)", leaflet >= 1)

    # 都道府県セレクタ
    pref_info = page.evaluate("""
        (function(){
            var sel = document.querySelector('select[name="prefecture"], select#prefecture, select[name="pref"]');
            if (!sel) {
                // fallback: あらゆるselectから47県それっぽいのを探す
                var sels = document.querySelectorAll('select');
                for (var i=0; i<sels.length; i++){
                    if (sels[i].options.length >= 40) { sel = sels[i]; break; }
                }
            }
            if (!sel) return {found: false};
            var opts = Array.from(sel.options).map(function(o){return o.textContent.trim();});
            return {
                found: true,
                count: opts.length,
                hasTokyo: opts.some(function(o){return o.indexOf('東京') >= 0;}),
                hasOkinawa: opts.some(function(o){return o.indexOf('沖縄') >= 0;}),
                hasHokkaido: opts.some(function(o){return o.indexOf('北海道') >= 0;})
            };
        })()
    """)
    info(f"都道府県セレクタ: {pref_info}")
    check("都道府県セレクタ存在", pref_info.get('found'))
    if pref_info.get('found'):
        check("47県含む (count>=47)", pref_info.get('count', 0) >= 47)
        check("東京/沖縄/北海道を含む",
              pref_info.get('hasTokyo') and pref_info.get('hasOkinawa') and pref_info.get('hasHokkaido'))

    # 検索ボタン
    search_btn = page.evaluate("""
        (function(){
            var btns = document.querySelectorAll('#content button, #content input[type=submit]');
            for (var i=0; i<btns.length; i++){
                var t = (btns[i].textContent || btns[i].value || '').trim();
                if (t.indexOf('検索') >= 0) return true;
            }
            return false;
        })()
    """)
    check("検索ボタン存在", bool(search_btn))

    bad = nan_check(page)
    check(f"NaN/undefined なし (detected: {bad.get('reasons')})", not bad.get('found'))


def test_analysis(page):
    """3. 詳細分析タブ"""
    print("\n=== [詳細分析タブ] ===")
    clicked = click_tab(page, "詳細")
    if not clicked:
        # "詳細分析" でなく "分析" の場合
        clicked = click_tab(page, "分析")
    if not clicked:
        check("詳細分析タブボタン検出", False)
        return
    text_len, sec = wait_content_loaded(page, min_len=500)
    info(f"タブ読み込み {text_len}文字 ({sec}秒)")
    check("タブ読み込み完了 (>500文字)", text_len > 500)
    ss(page, "03_analysis")

    body = page.text_content("#content") or ""

    # サブタブ3つ
    subtabs_found = sum(1 for s in ["構造", "トレンド", "総合"] if s in body)
    info(f"サブタブ候補ヒット数: {subtabs_found}")
    check("サブタブ3つ以上存在", subtabs_found >= 3)

    # 欠員率テーブル: パーセンテージの抽出
    percentages = page.evaluate("""
        (function(){
            var c = document.getElementById('content');
            if (!c) return [];
            var text = c.textContent || '';
            var matches = text.match(/([\\d.]+)\\s*%/g) || [];
            return matches.slice(0, 50).map(function(m){
                return parseFloat(m.replace('%','').trim());
            }).filter(function(v){return !isNaN(v);});
        })()
    """)
    info(f"パーセント値数: {len(percentages)} (先頭: {percentages[:5]})")
    if percentages:
        out_of_range = [v for v in percentages if v < 0 or v > 100]
        check(f"全%値が0-100範囲 (逸脱数: {len(out_of_range)})", len(out_of_range) == 0)
    else:
        info("%値が検出されなかった")
        check("パーセント値存在", False)

    # テーブル行数
    tbody_rows = page.evaluate(
        "document.querySelectorAll('#content table tbody tr').length"
    )
    info(f"テーブル行数合計: {tbody_rows}")
    check("欠員率テーブル行 >=3", tbody_rows >= 3)

    bad = nan_check(page)
    check(f"NaN/undefined なし (detected: {bad.get('reasons')})", not bad.get('found'))


def test_company(page):
    """4. 企業検索タブ"""
    print("\n=== [企業検索タブ] ===")
    clicked = click_tab(page, "企業")
    if not clicked:
        check("企業検索タブボタン検出", False)
        return
    text_len, sec = wait_content_loaded(page, min_len=200)
    info(f"タブ読み込み {text_len}文字 ({sec}秒)")
    check("タブ読み込み完了 (>200文字)", text_len > 200)
    ss(page, "04_company_initial")

    # 検索フォーム要素
    form_info = page.evaluate("""
        (function(){
            var root = document.getElementById('content');
            if (!root) return {};
            var inputs = root.querySelectorAll('input[type="text"], input[type="search"], input:not([type])');
            var selects = root.querySelectorAll('select');
            var buttons = root.querySelectorAll('button, input[type="submit"]');
            return {
                inputCount: inputs.length,
                selectCount: selects.length,
                buttonCount: buttons.length
            };
        })()
    """)
    info(f"フォーム要素: {form_info}")
    check("テキスト入力 >=1", form_info.get('inputCount', 0) >= 1)
    check("検索ボタン候補 >=1", form_info.get('buttonCount', 0) >= 1)

    # 「株式会社」で検索実行
    searched = page.evaluate("""
        (function(){
            var root = document.getElementById('content');
            if (!root) return 'no-content';
            var input = root.querySelector('input[type="text"], input[type="search"], input:not([type])');
            if (!input) return 'no-input';
            input.value = '株式会社';
            input.dispatchEvent(new Event('input', {bubbles: true}));
            input.dispatchEvent(new Event('change', {bubbles: true}));
            // formをsubmit
            var form = input.closest('form');
            if (form) {
                // HTMX経由なら hx-get/hx-post を発火
                if (typeof htmx !== 'undefined') {
                    htmx.trigger(form, 'submit');
                } else {
                    form.submit();
                }
                return 'submitted-form';
            }
            // ボタンクリック
            var btn = root.querySelector('button, input[type="submit"]');
            if (btn) { btn.click(); return 'clicked-button'; }
            return 'no-action';
        })()
    """)
    info(f"検索実行: {searched}")
    time.sleep(6)  # 結果待機
    ss(page, "04_company_result")

    # 結果テーブル
    result_rows = page.evaluate(
        "document.querySelectorAll('#content table tbody tr').length"
    )
    info(f"結果テーブル行数: {result_rows}")
    check("検索結果 >=10件", result_rows >= 10)

    # 列ヘッダ
    body = page.text_content("#content") or ""
    headers_ok = sum(1 for h in ["会社名", "企業名", "地域", "所在", "従業員"] if h in body)
    info(f"期待列ヘッダヒット: {headers_ok}")
    check("期待列ヘッダ >=2", headers_ok >= 2)

    bad = nan_check(page)
    check(f"NaN/undefined なし (detected: {bad.get('reasons')})", not bad.get('found'))


def test_diagnostic(page):
    """5. 条件診断タブ"""
    print("\n=== [条件診断タブ] ===")
    clicked = click_tab(page, "診断")
    if not clicked:
        clicked = click_tab(page, "条件")
    if not clicked:
        check("条件診断タブボタン検出", False)
        return
    text_len, sec = wait_content_loaded(page, min_len=200)
    info(f"タブ読み込み {text_len}文字 ({sec}秒)")
    check("タブ読み込み完了 (>200文字)", text_len > 200)
    ss(page, "05_diagnostic_initial")

    # 入力フォーム検出
    form_info = page.evaluate("""
        (function(){
            var root = document.getElementById('content');
            if (!root) return {};
            var numbers = root.querySelectorAll('input[type="number"], input[type="text"]');
            var fields = Array.from(numbers).map(function(i){
                return {
                    name: i.getAttribute('name') || '',
                    placeholder: i.getAttribute('placeholder') || '',
                    id: i.id || ''
                };
            });
            return {count: numbers.length, fields: fields.slice(0, 10)};
        })()
    """)
    info(f"入力フィールド: {form_info}")
    check("入力フィールド >=3", form_info.get('count', 0) >= 3)

    body = page.text_content("#content") or ""
    labels_ok = sum(1 for k in ["月給", "年間休日", "賞与", "休日"] if k in body)
    info(f"期待ラベルヒット: {labels_ok}")
    check("期待ラベル >=2", labels_ok >= 2)

    # 入力 → 診断実行
    filled = page.evaluate("""
        (function(){
            var root = document.getElementById('content');
            if (!root) return {ok: false, msg: 'no-content'};
            var inputs = root.querySelectorAll('input[type="number"], input[type="text"]');
            var values = [250000, 120, 2.0];
            var filled = 0;
            for (var i=0; i<inputs.length && i<3; i++){
                var inp = inputs[i];
                inp.value = values[i];
                inp.dispatchEvent(new Event('input', {bubbles: true}));
                inp.dispatchEvent(new Event('change', {bubbles: true}));
                filled++;
            }
            // 診断ボタンを探してクリック
            var btns = root.querySelectorAll('button, input[type="submit"]');
            var clicked = null;
            for (var j=0; j<btns.length; j++){
                var t = (btns[j].textContent || btns[j].value || '').trim();
                if (t.indexOf('診断') >= 0 || t.indexOf('判定') >= 0 || t.indexOf('送信') >= 0) {
                    btns[j].click();
                    clicked = t;
                    break;
                }
            }
            if (!clicked && btns.length > 0) {
                btns[0].click();
                clicked = 'first-button';
            }
            return {ok: true, filled: filled, clicked: clicked};
        })()
    """)
    info(f"入力&クリック: {filled}")
    time.sleep(6)
    ss(page, "05_diagnostic_result")

    # グレード表示
    body = page.text_content("#content") or ""
    grade_match = re.search(r'(?:グレード|評価|ランク|判定)[^A-Z]{0,20}([A-Da-d])', body)
    found_any_grade = bool(re.search(r'\b([A-D])\s*(?:グレード|ランク|評価|級|判定)', body)) \
                      or bool(grade_match)
    if grade_match:
        info(f"抽出グレード: {grade_match.group(1).upper()}")
    else:
        # fallback: 単独 A/B/C/D タグ
        single = re.findall(r'(?<![A-Za-z0-9])([A-D])(?![A-Za-z0-9])', body)
        info(f"単独A-D出現数: {len(single)}")
        found_any_grade = found_any_grade or len(single) > 0
    check("グレードA/B/C/Dのいずれか表示", found_any_grade)

    bad = nan_check(page)
    check(f"NaN/undefined なし (detected: {bad.get('reasons')})", not bad.get('found'))


# =========================================================
# main
# =========================================================

def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, slow_mo=300)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        console_errors = []
        page.on("console", lambda m: console_errors.append(m.text) if m.type == "error" else None)

        # === ログイン ===
        print("\n=== [ログイン] ===")
        page.goto(BASE, timeout=60000)
        time.sleep(3)
        page.fill('input[name="email"]', EMAIL)
        page.fill('input[name="password"]', PASSWORD)
        page.click('button[type="submit"]')
        time.sleep(8)
        logged_in = "ログアウト" in (page.text_content("body") or "")
        check("ログイン成功", logged_in)
        if not logged_in:
            print("  [ABORT] ログイン失敗のため中断")
            browser.close()
            return

        # htmx ロード待機
        try:
            page.wait_for_function("typeof htmx !== 'undefined'", timeout=15000)
        except Exception:
            info("htmx wait timeout (continuing)")
        time.sleep(1)

        # === 各タブ ===
        try:
            test_market(page)
        except Exception as e:
            print(f"  [EXCEPTION] 市場概況: {type(e).__name__}: {e}")
            check("市場概況タブ例外なし", False)

        try:
            test_jobmap(page)
        except Exception as e:
            print(f"  [EXCEPTION] 地図: {type(e).__name__}: {e}")
            check("地図タブ例外なし", False)

        try:
            test_analysis(page)
        except Exception as e:
            print(f"  [EXCEPTION] 詳細分析: {type(e).__name__}: {e}")
            check("詳細分析タブ例外なし", False)

        try:
            test_company(page)
        except Exception as e:
            print(f"  [EXCEPTION] 企業検索: {type(e).__name__}: {e}")
            check("企業検索タブ例外なし", False)

        try:
            test_diagnostic(page)
        except Exception as e:
            print(f"  [EXCEPTION] 条件診断: {type(e).__name__}: {e}")
            check("条件診断タブ例外なし", False)

        # === コンソールエラー ===
        print("\n=== [コンソールエラー] ===")
        real_errors = [
            e for e in console_errors
            if "favicon" not in e.lower()
            and "manifest" not in e.lower()
            and "404" not in e.lower()
        ]
        info(f"致命的エラー数: {len(real_errors)}")
        for e in real_errors[:5]:
            print(f"    - {e[:200]}")
        check("致命的なコンソールエラーなし", len(real_errors) == 0)

        # === サマリー ===
        print("\n" + "=" * 50)
        print(f"合計: {TOTAL}テスト / PASS: {PASSED} / FAIL: {FAILED}")
        print("=" * 50)

        time.sleep(2)
        browser.close()


if __name__ == "__main__":
    main()
