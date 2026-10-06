"""Verify production-generated reports against the saved trial inputs, in Edge."""
import json
import math
from pathlib import Path
from playwright.sync_api import sync_playwright

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / 'target/app-report-manifest.json').read_text(encoding='utf-8'))
results = {}
with sync_playwright() as p:
    browser = p.chromium.launch(channel='msedge', headless=True)
    for case in manifest['cases']:
        folder = Path(manifest['output_dir']) / case['slug']
        page = browser.new_page(viewport={'width': 1600, 'height': 1100})
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.goto((folder / 'report.html').as_uri())
        page.wait_for_timeout(300)
        for index, series in enumerate(['keyword-all', 'keyword-head']):
            data = json.loads(page.locator('#' + series).text_content())
            assert len(data['rows']) == 20
            expected = {row['word']: row for row in data['rows']}
            chart = page.locator(f'svg[data-series="{series}"]')
            bars = chart.locator('rect[data-word]').evaluate_all('(els)=>els.map(e=>({word:e.dataset.word,group:e.dataset.group,value:+e.dataset.value,y:+e.getAttribute("y")}))')
            assert len(bars) == (20 if index == 0 else 40)
            for bar in bars:
                group = 'all' if bar['group'] == '全体' else 'head'
                value = expected[bar['word']][group]
                if index:
                    value = value / data['allN' if group == 'all' else 'headN'] * 100
                assert math.isclose(bar['value'], value, abs_tol=1e-8)
            assert max(bar['y'] for bar in bars) - min(bar['y'] for bar in bars) > chart.bounding_box()['height'] * .75
        page.locator('#tab-indeed').click()
        charts = page.locator('#panel-indeed .trend-grid svg')
        assert charts.count() == 4
        for i, key in enumerate(['job', 'ctk', 'emp', 'spp']):
            points = charts.nth(i).locator('circle[data-month]').evaluate_all('(els)=>els.map(e=>({month:e.dataset.month,value:+e.dataset.value}))')
            assert len(points) == len(case['indeed']['rows'])
            for point, row in zip(points, case['indeed']['rows']):
                assert point['month'] == row['month']
                assert math.isclose(point['value'], row[key], rel_tol=1e-14, abs_tol=1e-12)
        page.locator('#tab-google').click()
        points = page.locator('#panel-google circle[data-month]').evaluate_all('(els)=>els.map(e=>({month:e.dataset.month,value:+e.dataset.value}))')
        assert points == [{'month': row['month'], 'value': row['search_volume']} for row in case['google']['demand']['keywords'][0]['monthly_12m']]
        page.locator('#tab-population').click()
        population = case['population']
        total = population['totals']['total_population']
        expected = {band['age_group']: band for band in population['bands']}
        bars = page.locator('[data-age][data-sex]').evaluate_all('(els)=>els.map(e=>({...e.dataset}))')
        assert len(bars) == 18
        for bar in bars:
            count = expected[bar['age']][bar['sex']]
            assert int(bar['count']) == count
            assert math.isclose(float(bar['share']), count / total * 100, abs_tol=1e-8)
        assert page.locator('#panel-population').inner_text().count('126,146,099') == 2
        page.locator('#panel-population').screenshot(path=str(folder / 'population.png'))
        for width in [320, 900, 1600]:
            page.set_viewport_size({'width': width, 'height': 1100})
            for tab in ['excel', 'google', 'indeed', 'population', 'consultation']:
                page.locator('#tab-' + tab).click()
                page.wait_for_timeout(100)
                assert page.locator('[role=tabpanel]:visible').count() == 1
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
                if tab == 'excel':
                    assert page.evaluate('document.body.scrollHeight') < 5000
        page.locator('#tab-excel').focus()
        page.keyboard.press('End')
        assert page.locator('#panel-consultation').is_visible()
        page.keyboard.press('Home')
        page.wait_for_timeout(200)
        assert page.locator('#panel-excel').is_visible()
        page.screenshot(path=str(folder / 'screen.png'), full_page=True)
        assert not errors, errors
        results[case['slug']] = {'keywords': [20, 20], 'indeed_graphs': 4, 'google_points': 12, 'population': total, 'widths': [320, 900, 1600]}
        page.close()
    browser.close()
Path(manifest['output_dir'], 'visual-verification.json').write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(results, ensure_ascii=False))
