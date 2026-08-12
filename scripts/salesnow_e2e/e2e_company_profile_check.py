# -*- coding: utf-8 -*-
"""企業検索タブの「地域×業種 人材フロー比較」を E2E で検証する。

表示されるケースと、抑制されるケース (企業数不足 / 1 社集中) の両方を、
実際の企業プロフィール HTML で確認する。
"""
import http.cookiejar
import os
import re
import sqlite3
import urllib.parse
import urllib.request

BASE = os.environ.get("BASE", "http://127.0.0.1:9311")
DB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "e2e_salesnow.db")

cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
opener.open(BASE + "/login", urllib.parse.urlencode(
    {"email": "s_fujimaki@f-a-c.co.jp", "password": "e2e-local-test"}).encode()).read()

c = sqlite3.connect(DB)
CELL_SQL = """
SELECT prefecture, sn_industry,
       COUNT(*) n,
       CAST(SUM(ROUND(employee_count*employee_delta_1y/(100.0+employee_delta_1y))) AS INTEGER) net,
       CAST(SUM(ABS(ROUND(employee_count*employee_delta_1y/(100.0+employee_delta_1y)))) AS INTEGER) abs_chg,
       CAST(MAX(ABS(ROUND(employee_count*employee_delta_1y/(100.0+employee_delta_1y)))) AS INTEGER) top1
FROM v2_salesnow_companies
WHERE employee_count > 0 AND employee_delta_1y IS NOT NULL AND employee_delta_1y > -100
  AND sn_industry IS NOT NULL AND sn_industry != ''
GROUP BY prefecture, sn_industry
"""
cells = c.execute(CELL_SQL).fetchall()


def pick(pred):
    for pref, ind, n, net, abs_chg, top1 in cells:
        share = (top1 / abs_chg * 100) if abs_chg else None
        if not pred(n, share):
            continue
        row = c.execute(
            """SELECT corporate_number, company_name FROM v2_salesnow_companies
               WHERE prefecture=? AND sn_industry=? AND employee_count>50
                 AND employee_delta_1y IS NOT NULL
               ORDER BY employee_count DESC LIMIT 1""", (pref, ind)).fetchone()
        if row:
            return pref, ind, n, share, row[0], row[1]
    return None


CASES = [
    ("表示されるはず", lambda n, s: n >= 30 and s is not None and s < 50, True),
    ("1社集中で抑制されるはず", lambda n, s: n >= 30 and s is not None and s >= 50, False),
    ("企業数不足で抑制されるはず", lambda n, s: n < 30, False),
]

fails = 0
for label, pred, should_show in CASES:
    got = pick(pred)
    if not got:
        print(f"[skip] {label}: 該当セルが無い")
        continue
    pref, ind, n, share, cn, name = got
    with opener.open(f"{BASE}/api/company/profile/{cn}", timeout=180) as r:
        html = r.read().decode("utf-8")
    i = html.find("人材フロー比較")
    seg = html[i:i + 1600] if i >= 0 else ""
    txt = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", " ", seg)).strip()

    share_s = f"{share:.1f}%" if share is not None else "-"
    print(f"\n=== {label} ===")
    print(f"  {pref} × {ind} (n={n}, 最大1社={share_s})  企業: {name}")
    if not seg:
        print("  [NG] 比較セクションが描画されていない")
        fails += 1
        continue
    print(f"  描画: {txt[:220]}")

    has_dash = "—" in seg or "&mdash;" in seg
    has_nocmp = "比較なし" in seg
    has_rate = re.search(r"増減率\s*[+-]\d", txt) is not None
    if should_show:
        if not has_rate:
            print("  [NG] 増減率が出ていない")
            fails += 1
        elif has_nocmp:
            print("  [NG] 表示すべきなのに『比較なし』")
            fails += 1
        else:
            print("  [OK] 増減率が表示され、比較も出ている")
    else:
        if has_rate:
            print("  [NG] 抑制すべきなのに増減率が出ている")
            fails += 1
        elif not (has_dash and has_nocmp):
            print(f"  [NG] 抑制表示が不十分 (—:{has_dash} 比較なし:{has_nocmp})")
            fails += 1
        else:
            print("  [OK] 値を伏せ、理由と『比較なし』が出ている")

    # 営業提案文に地域比較が混入していないか (抑制時)
    if not should_show:
        for bad in ["ポイント下回っています", "ポイント上回る成長率"]:
            if bad in html:
                print(f"  [NG] 抑制セルなのに営業提案文に地域比較が出ている: {bad}")
                fails += 1

    # 旧実装の痕跡
    for bad in ["地域平均比", "NaN", "inf"]:
        if bad in seg:
            print(f"  [NG] 旧実装/壊れた値の痕跡: {bad}")
            fails += 1

print(f"\n{'=' * 60}\n不合格: {fails} 件")
