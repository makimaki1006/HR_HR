# -*- coding: utf-8 -*-
"""Cycle 3: 全 47 都道府県 × 全業種を API 経由で取得し、
CSV から独立に計算した期待値と全件突き合わせる。

検証するのは Rust (region_headcount) + SQL + HTTP を通した最終出力。
期待値は pandas で SQL を使わずに計算する (実装の写しにならないようにする)。
"""
import http.cookiejar
import json
import os
import urllib.parse
import urllib.request

import numpy as np
import pandas as pd

BASE = os.environ.get("BASE", "http://127.0.0.1:9311")
CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MIN_COMPANIES = 30
MAX_TOP1_SHARE = 50.0

PREFS = ["北海道","青森県","岩手県","宮城県","秋田県","山形県","福島県","茨城県","栃木県","群馬県",
         "埼玉県","千葉県","東京都","神奈川県","新潟県","富山県","石川県","福井県","山梨県","長野県",
         "岐阜県","静岡県","愛知県","三重県","滋賀県","京都府","大阪府","兵庫県","奈良県","和歌山県",
         "鳥取県","島根県","岡山県","広島県","山口県","徳島県","香川県","愛媛県","高知県","福岡県",
         "佐賀県","長崎県","熊本県","大分県","宮崎県","鹿児島県","沖縄県"]

cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
opener.open(BASE + "/login", urllib.parse.urlencode(
    {"email": "s_fujimaki@f-a-c.co.jp", "password": "e2e-local-test"}).encode()).read()

print("CSV から期待値を独立計算中...")
df = pd.read_csv(CSV, usecols=["prefecture", "sn_industry", "employee_count",
                               "employee_delta_1y"], low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ok = ec.notna() & (ec > 0) & d.notna() & (d > -100) & df["sn_industry"].notna() \
     & (df["sn_industry"].astype(str) != "")
w = df[ok].copy()
w["ec"] = ec[ok]
w["d"] = d[ok]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
g = w.groupby(["prefecture", "sn_industry"])
exp = g.agg(companies=("d", "size"), total_emp=("ec", "sum"), net=("chg", "sum"))
exp["abs_chg"] = g["chg"].apply(lambda s: s.abs().sum())
exp["top1"] = g["chg"].apply(lambda s: s.abs().max())
exp = exp.astype({"companies": int, "total_emp": int, "net": int, "abs_chg": int, "top1": int})

mismatch = []
checked = shown = suppressed_few = suppressed_conc = 0
cells_missing_api = cells_extra_api = 0

for pref in PREFS:
    q = urllib.parse.urlencode({"prefecture": pref, "municipality": ""})
    with opener.open(f"{BASE}/api/jobmap/labor-flow?{q}", timeout=180) as r:
        data = json.loads(r.read().decode("utf-8"))
    if data.get("error"):
        print(f"  {pref}: error {data['error']}")
        continue
    api = {i["sn_industry"]: i for i in data.get("industries", [])}
    try:
        e_pref = exp.loc[pref]
    except KeyError:
        e_pref = exp.iloc[0:0]

    for ind in set(api) | set(e_pref.index):
        if ind not in api:
            cells_missing_api += 1
            mismatch.append((pref, ind, "API に無い", "-", "-"))
            continue
        if ind not in e_pref.index:
            cells_extra_api += 1
            mismatch.append((pref, ind, "期待値に無い", "-", "-"))
            continue
        a = api[ind]
        e = e_pref.loc[ind]
        checked += 1

        for key, got, want in [
            ("companies", a["companies"], int(e["companies"])),
            ("total_emp", a["total_emp"], int(e["total_emp"])),
            ("net_change_1y", a["net_change_1y"], int(e["net"])),
        ]:
            if got != want:
                mismatch.append((pref, ind, key, got, want))

        # top1_share_pct
        want_share = (e["top1"] / e["abs_chg"] * 100) if e["abs_chg"] > 0 else None
        got_share = a["top1_share_pct"]
        if want_share is None:
            if got_share is not None:
                mismatch.append((pref, ind, "top1_share_pct", got_share, None))
        elif got_share is None or abs(got_share - want_share) > 1e-6:
            mismatch.append((pref, ind, "top1_share_pct", got_share, want_share))

        # ゲート判定と表示値
        past = int(e["total_emp"]) - int(e["net"])
        few = int(e["companies"]) < MIN_COMPANIES
        conc = want_share is not None and want_share >= MAX_TOP1_SHARE
        want_rate = None if (few or conc or past <= 0) else e["net"] / past * 100
        got_rate = a["headcount_rate_1y"]
        if want_rate is None:
            if got_rate is not None:
                mismatch.append((pref, ind, "抑制すべきなのに値が出た", got_rate, None))
            if not a["headcount_notice"]:
                mismatch.append((pref, ind, "抑制時に notice が無い", None, "必要"))
            if few:
                suppressed_few += 1
            elif conc:
                suppressed_conc += 1
        else:
            if got_rate is None:
                mismatch.append((pref, ind, "表示すべきなのに抑制された", None, want_rate))
            elif abs(got_rate - want_rate) > 1e-6:
                mismatch.append((pref, ind, "headcount_rate_1y", got_rate, want_rate))
            elif a["headcount_notice"] is not None:
                mismatch.append((pref, ind, "表示時に notice が付いている", a["headcount_notice"], None))
            else:
                shown += 1

print()
print("=" * 74)
print(f"照合したセル数        : {checked:,}")
print(f"  表示された          : {shown:,}")
print(f"  企業数不足で抑制    : {suppressed_few:,}")
print(f"  1 社集中で抑制      : {suppressed_conc:,}")
print(f"API にしか無いセル    : {cells_extra_api}")
print(f"期待値にしか無いセル  : {cells_missing_api}")
print(f"不一致                : {len(mismatch)}")
print("=" * 74)
if mismatch:
    print("\n不一致の詳細 (先頭 25 件):")
    print(f"  {'都道府県':<8} {'業種':<16} {'項目':<28} {'API':>14} {'期待':>14}")
    for m in mismatch[:25]:
        print(f"  {str(m[0]):<8} {str(m[1])[:14]:<16} {str(m[2])[:26]:<28} "
              f"{str(m[3])[:12]:>14} {str(m[4])[:12]:>14}")
else:
    print("\n全セルで API 出力と独立計算が一致した。")
