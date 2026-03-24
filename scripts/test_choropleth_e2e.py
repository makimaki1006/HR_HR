"""
choropleth_overlay E2E Test - Production HelloWork Dashboard
=====================================================================
Tests the choropleth data overlay feature on the jobmap tab.
Phases: Setup -> API Validation -> UI Layer Switching -> Reset -> Prefecture Switch -> Console Errors
"""

import sys
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

BASE_URL = "https://hr-hw.onrender.com"
EMAIL = "test@cyxen.co.jp"
PASSWORD = "cyxen_2025"

SCREENSHOT_DIR = Path(r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\scripts\screenshots\prod_choropleth")
SCREENSHOT_DIR.mkdir(parents=True, exist_ok=True)

LAYERS = [
    ("posting_count", "求人件数"),
    ("avg_salary", "平均給与"),
    ("day_night_ratio", "昼夜間人口比"),
    ("net_migration", "転入超過率"),
    ("vacancy_rate", "欠員補充率"),
    ("min_wage", "最低賃金"),
]

results = []
console_errors = []

# GeoJSONレイヤー数のカウント用JS (タイルレイヤーを除外する精密な判定)
COUNT_GEO_LAYERS_JS = """
    (() => {
        var count = 0;
        if (window.__jmMap) {
            window.__jmMap.eachLayer(function(l) {
                // GeoJSONレイヤーは feature プロパティを持つか、
                // _layers を持ちかつ L.GeoJSON のインスタンス
                if (l.feature) {
                    count++;
                } else if (l._layers && typeof l.getBounds === 'function' && !l._url) {
                    // _url がないのはタイルレイヤーではないことの目安
                    count++;
                }
            });
        }
        return count;
    })()
"""


def shot(page, name):
    path = SCREENSHOT_DIR / f"{name}.png"
    page.screenshot(path=str(path), full_page=False)
    return str(path)


def record(test_id, desc, passed, detail=""):
    tag = "[PASS]" if passed else "[FAIL]"
    msg = f"{tag} {test_id} {desc}"
    if detail:
        msg += f" -- {detail}"
    print(msg)
    results.append((test_id, desc, passed, detail))


def load_jobmap_tab(page):
    """htmx.ajaxでjobmapタブをロードし、地図初期化を待つ"""
    page.evaluate("""
        new Promise((resolve, reject) => {
            htmx.ajax('GET', '/tab/jobmap', {target: '#content', swap: 'innerHTML'}).then(resolve).catch(reject);
        })
    """)
    try:
        page.wait_for_selector("#jm-map", timeout=20000)
    except Exception:
        pass
    time.sleep(12)


def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        context = browser.new_context(
            viewport={"width": 1400, "height": 900},
            ignore_https_errors=True,
        )
        page = context.new_page()

        # Collect console messages related to choropleth/map/errors
        page.on("console", lambda msg: (
            console_errors.append(f"[{msg.type}] {msg.text}")
            if msg.type in ("error", "warning") and any(
                kw in msg.text.lower()
                for kw in ["choropleth", "leaflet", "map", "geojson", "uncaught",
                            "typeerror", "referenceerror"]
            )
            else None
        ))

        # ============================================================
        # Phase 1: Setup
        # ============================================================
        print("\n=== Phase 1: Setup ===")

        # C-1: Login
        try:
            page.goto(f"{BASE_URL}/login", timeout=60000, wait_until="networkidle")
            page.fill('input[name="email"]', EMAIL)
            page.fill('input[name="password"]', PASSWORD)
            page.click('button[type="submit"]')
            page.wait_for_url(f"{BASE_URL}/", timeout=30000)
            time.sleep(3)
            shot(page, "01_login_success")
            record("C-1", "Login", True)
        except Exception as e:
            shot(page, "01_login_fail")
            record("C-1", "Login", False, str(e))
            browser.close()
            return 1

        # C-2: Select 東京都
        try:
            page.wait_for_selector("#pref-select", timeout=15000)
            page.select_option("#pref-select", label="東京都")
            time.sleep(3)
            shot(page, "02_pref_tokyo")
            record("C-2", "Select 東京都", True)
        except Exception as e:
            shot(page, "02_pref_fail")
            record("C-2", "Select 東京都", False, str(e))

        # C-3: Load jobmap tab
        try:
            load_jobmap_tab(page)
            shot(page, "03_jobmap_initial")
            has_map = page.evaluate("document.getElementById('jm-map') !== null")
            has_dropdown = page.evaluate("document.getElementById('jm-choropleth-layer') !== null")
            record("C-3", "Load jobmap tab", has_map and has_dropdown,
                   f"jm-map={has_map}, dropdown={has_dropdown}")
        except Exception as e:
            shot(page, "03_jobmap_fail")
            record("C-3", "Load jobmap tab", False, str(e))

        # C-4: Verify dropdown has 7 options
        try:
            options = page.evaluate("""
                Array.from(document.querySelectorAll('#jm-choropleth-layer option'))
                     .map(o => ({value: o.value, text: o.textContent.trim()}))
            """)
            count = len(options)
            record("C-4", "Choropleth dropdown: 7 options",
                   count == 7,
                   f"options={count}, values={[o['value'] for o in options]}")
        except Exception as e:
            record("C-4", "Choropleth dropdown: 7 options", False, str(e))

        # ============================================================
        # Phase 2: API Data Validation (for each of 6 layers)
        # ============================================================
        print("\n=== Phase 2: API Data Validation ===")

        for idx, (layer_id, layer_name) in enumerate(LAYERS, start=1):
            test_id = f"C-5.{idx}"
            try:
                api_result = page.evaluate(f"""
                    (async () => {{
                        const resp = await fetch('/api/jobmap/choropleth?layer={layer_id}&prefecture=' + encodeURIComponent('東京都'));
                        return await resp.json();
                    }})()
                """)

                choropleth = api_result.get("choropleth", {})
                legend = api_result.get("legend", [])
                geojson_url = api_result.get("geojsonUrl", "")
                error_msg = api_result.get("error", "")

                muni_count = len(choropleth)
                has_legend = len(legend) > 0
                has_geojson = bool(geojson_url)

                # Validate structure of each choropleth entry
                structure_ok = True
                sample_info = ""
                if muni_count > 0:
                    first_key = list(choropleth.keys())[0]
                    fv = choropleth[first_key]
                    structure_ok = all(k in fv for k in ("fillColor", "fillOpacity", "value"))
                    sample_info = (f"sample=({first_key}: value={fv.get('value','?')}, "
                                   f"fill={fv.get('fillColor','?')}, "
                                   f"opacity={fv.get('fillOpacity','?')})")

                # Validate legend structure
                legend_ok = all("color" in item and "label" in item for item in legend) if has_legend else False

                passed = (muni_count > 0 and has_legend and has_geojson
                          and structure_ok and legend_ok and not error_msg)
                detail = (f"municipalities={muni_count}, legend={len(legend)}(valid={legend_ok}), "
                          f"geojsonUrl={'yes' if has_geojson else 'no'}, "
                          f"structureOk={structure_ok}, {sample_info}")
                if error_msg:
                    detail += f", ERROR={error_msg}"
                record(test_id, f"API: {layer_name} ({layer_id})", passed, detail)
            except Exception as e:
                record(test_id, f"API: {layer_name} ({layer_id})", False, str(e))

        # ============================================================
        # Phase 3: UI Layer Switching (for each of 6 layers)
        # ============================================================
        print("\n=== Phase 3: UI Layer Switching ===")

        dropdown_exists = page.evaluate("document.getElementById('jm-choropleth-layer') !== null")

        # Get baseline layer count (before any choropleth layer is added)
        baseline_geo_layers = page.evaluate(COUNT_GEO_LAYERS_JS)
        print(f"  Baseline geoLayers (before any choropleth): {baseline_geo_layers}")

        for idx, (layer_id, layer_name) in enumerate(LAYERS, start=1):
            test_id = f"C-6.{idx}"
            try:
                if not dropdown_exists:
                    record(test_id, f"UI: {layer_name} ({layer_id})", False, "dropdown not in DOM")
                    continue

                # Select layer via JS
                page.evaluate(f"""
                    (() => {{
                        var sel = document.getElementById('jm-choropleth-layer');
                        if (!sel) return;
                        sel.value = '{layer_id}';
                        sel.dispatchEvent(new Event('change', {{bubbles: true}}));
                        if (typeof choroplethOverlay !== 'undefined') {{
                            choroplethOverlay.switchLayer('{layer_id}');
                        }}
                    }})()
                """)
                time.sleep(8)

                # Map reference
                map_ref_type = page.evaluate("""
                    (() => {
                        if (window.__jmMap && typeof window.__jmMap.addLayer === 'function') return 'window.__jmMap';
                        var el = document.getElementById('jm-map');
                        if (!el) return 'no-el';
                        if (el._leaflet_id) return 'leaflet_id=' + el._leaflet_id;
                        return 'no-ref';
                    })()
                """)

                # Count overlay SVG paths
                overlay_paths = page.evaluate(
                    "document.querySelectorAll('#jm-map .leaflet-overlay-pane path').length")

                # Count GeoJSON layers (excluding tiles)
                geo_layers = page.evaluate(COUNT_GEO_LAYERS_JS)

                # Legend info
                legend_info = page.evaluate("""
                    (() => {
                        var el = document.getElementById('jm-choropleth-legend');
                        if (!el) return {exists: false, visible: false, display: 'N/A', text: ''};
                        var style = window.getComputedStyle(el);
                        var text = el.innerText || el.textContent || '';
                        return {
                            exists: true,
                            display: style.display,
                            visible: style.display !== 'none' && text.trim().length > 0,
                            text: text.substring(0, 200)
                        };
                    })()
                """)

                shot(page, f"04_layer_{layer_id}")

                legend_visible = legend_info.get("visible", False)
                legend_text = legend_info.get("text", "")

                # PASS: legend visible AND GeoJSON layer was added (more layers than baseline)
                passed = legend_visible and geo_layers > baseline_geo_layers
                detail = (f"mapRef={map_ref_type}, overlayPaths={overlay_paths}, "
                          f"geoLayers={geo_layers}(baseline={baseline_geo_layers}), "
                          f"legendVisible={legend_visible}, legendText='{legend_text[:60]}...'")
                record(test_id, f"UI: {layer_name} ({layer_id})", passed, detail)
            except Exception as e:
                shot(page, f"04_layer_{layer_id}_fail")
                record(test_id, f"UI: {layer_name} ({layer_id})", False, str(e))

        # ============================================================
        # Phase 4: Layer Reset
        # ============================================================
        print("\n=== Phase 4: Layer Reset ===")
        try:
            page.evaluate("""
                (() => {
                    var sel = document.getElementById('jm-choropleth-layer');
                    if (!sel) return;
                    sel.value = '';
                    sel.dispatchEvent(new Event('change', {bubbles: true}));
                    if (typeof choroplethOverlay !== 'undefined') {
                        choroplethOverlay.switchLayer('');
                    }
                })()
            """)
            time.sleep(3)

            overlay_paths_after = page.evaluate(
                "document.querySelectorAll('#jm-map .leaflet-overlay-pane path').length")
            geo_layers_after = page.evaluate(COUNT_GEO_LAYERS_JS)
            legend_hidden = page.evaluate("""
                (() => {
                    var el = document.getElementById('jm-choropleth-legend');
                    if (!el) return true;
                    var style = window.getComputedStyle(el);
                    return style.display === 'none' || (el.innerText||'').trim().length === 0;
                })()
            """)

            shot(page, "05_layer_reset")
            # After reset, geoLayers should return to baseline (no choropleth GeoJSON layer)
            passed = legend_hidden and geo_layers_after <= baseline_geo_layers
            detail = (f"overlayPaths={overlay_paths_after}, "
                      f"geoLayers={geo_layers_after}(baseline={baseline_geo_layers}), "
                      f"legendHidden={legend_hidden}")
            record("C-7", "Layer reset (none)", passed, detail)
        except Exception as e:
            shot(page, "05_layer_reset_fail")
            record("C-7", "Layer reset (none)", False, str(e))

        # ============================================================
        # Phase 5: Prefecture Switch (大阪府)
        # ============================================================
        print("\n=== Phase 5: Prefecture Switch ===")
        try:
            page.evaluate("""
                (() => {
                    var sel = document.getElementById('pref-select');
                    if (!sel) return;
                    sel.value = '大阪府';
                    sel.dispatchEvent(new Event('change', {bubbles: true}));
                })()
            """)
            time.sleep(3)

            load_jobmap_tab(page)
            shot(page, "06_osaka_jobmap")

            # Select posting_count
            page.evaluate("""
                (() => {
                    var sel = document.getElementById('jm-choropleth-layer');
                    if (!sel) return;
                    sel.value = 'posting_count';
                    sel.dispatchEvent(new Event('change', {bubbles: true}));
                    if (typeof choroplethOverlay !== 'undefined') {
                        choroplethOverlay.switchLayer('posting_count');
                    }
                })()
            """)
            time.sleep(8)

            # API verification
            osaka_api = page.evaluate("""
                (async () => {
                    const resp = await fetch('/api/jobmap/choropleth?layer=posting_count&prefecture=' + encodeURIComponent('大阪府'));
                    return await resp.json();
                })()
            """)
            osaka_pref = osaka_api.get("prefecture", "")
            osaka_choropleth = osaka_api.get("choropleth", {})
            osaka_count = len(osaka_choropleth)
            osaka_geojson = osaka_api.get("geojsonUrl", "")

            osaka_sample = ""
            if osaka_count > 0:
                first_key = list(osaka_choropleth.keys())[0]
                osaka_sample = f"{first_key}={osaka_choropleth[first_key].get('value','?')}"

            # Check UI legend
            osaka_legend_vis = page.evaluate("""
                (() => {
                    var el = document.getElementById('jm-choropleth-legend');
                    if (!el) return false;
                    return window.getComputedStyle(el).display !== 'none';
                })()
            """)

            shot(page, "07_osaka_posting_count")
            passed = osaka_count > 0 and osaka_pref == "大阪府"
            detail = (f"prefecture={osaka_pref}, municipalities={osaka_count}, "
                      f"geojsonUrl={osaka_geojson}, sample={osaka_sample}, "
                      f"legendVisible={osaka_legend_vis}")
            record("C-8", "Prefecture switch to 大阪府", passed, detail)
        except Exception as e:
            shot(page, "07_osaka_fail")
            record("C-8", "Prefecture switch to 大阪府", False, str(e))

        # ============================================================
        # Phase 6: Console Error Check
        # ============================================================
        print("\n=== Phase 6: Console Error Check ===")
        error_msgs = [e for e in console_errors if "[error]" in e.lower()]
        warning_msgs = [e for e in console_errors if "[warning]" in e.lower()]
        if error_msgs:
            detail = "; ".join(error_msgs[:5])
            record("C-9", "No critical console errors",
                   len(error_msgs) <= 2,
                   f"{len(error_msgs)} errors, {len(warning_msgs)} warnings: {detail}")
        else:
            record("C-9", "No critical console errors", True,
                   f"0 errors, {len(warning_msgs)} warnings captured")

        # ============================================================
        # Summary
        # ============================================================
        browser.close()

        print("\n" + "=" * 70)
        total = len(results)
        passed_count = sum(1 for r in results if r[2])
        failed_count = total - passed_count
        print(f"Summary: {passed_count}/{total} passed, {failed_count} failed")
        print("=" * 70)

        if failed_count > 0:
            print("\nFailed tests:")
            for tid, desc, p_, d in results:
                if not p_:
                    print(f"  [FAIL] {tid} {desc} -- {d}")

        if console_errors:
            print(f"\nAll console messages ({len(console_errors)}):")
            for e in console_errors[:20]:
                print(f"  {e}")

        return 0 if failed_count == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
