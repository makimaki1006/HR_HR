# -*- coding: utf-8 -*-
"""E2E: 稼働中の Rust サーバの /api/jobmap/labor-flow を叩き、
抑制ゲートの効き具合と実用性 (どれだけ値が出るか) を測る。
"""
import json
import os
import sys
import urllib.parse
import urllib.request
import http.cookiejar

BASE = os.environ.get("BASE", "http://127.0.0.1:9311")
EMAIL = "s_fujimaki@f-a-c.co.jp"
PW = "e2e-local-test"

cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
opener.open(BASE + "/login",
            urllib.parse.urlencode({"email": EMAIL, "password": PW}).encode()).read()


def labor_flow(pref, muni=""):
    q = urllib.parse.urlencode({"prefecture": pref, "municipality": muni})
    with opener.open(f"{BASE}/api/jobmap/labor-flow?{q}", timeout=120) as r:
        return json.loads(r.read().decode("utf-8"))


TARGETS = [
    ("東京都", ""), ("東京都", "千代田区"), ("東京都", "港区"), ("東京都", "府中市"),
    ("広島県", "福山市"), ("大阪府", "東大阪市"), ("山梨県", "南都留郡道志村"),
    ("愛知県", "名古屋市中川区"), ("島根県", "隠岐郡海士町"), ("北海道", "札幌市白石区"),
]

print(f"{'地域':<24} {'業種':>5} {'表示':>5} {'不足':>5} {'集中':>5} {'企業カバー率':>12}")
print("-" * 68)
tot_ind = tot_shown = 0
for pref, muni in TARGETS:
    try:
        d = labor_flow(pref, muni)
    except Exception as e:
        print(f"{pref+' '+muni:<24} ERROR {e}")
        continue
    if d.get("error"):
        print(f"{pref+' '+muni:<24} error: {d['error']}")
        continue
    ind = d.get("industries", [])
    shown = [i for i in ind if i["headcount_rate_1y"] is not None]
    few = [i for i in ind if i["headcount_notice"] and "社に満たない" in i["headcount_notice"]]
    conc = [i for i in ind if i["headcount_notice"] and "1 社が占め" in i["headcount_notice"]]
    tot = sum(i["companies"] for i in ind)
    cov = sum(i["companies"] for i in shown)
    tot_ind += len(ind)
    tot_shown += len(shown)
    label = f"{pref} {muni}".strip()
    print(f"{label:<24} {len(ind):>5} {len(shown):>5} {len(few):>5} {len(conc):>5} "
          f"{cov:>7,}/{tot:<7,} {cov/max(tot,1)*100:>5.1f}%")

print("-" * 68)
print(f"合計: 業種セル {tot_ind} 件中 {tot_shown} 件表示 ({tot_shown/max(tot_ind,1)*100:.1f}%)")

# 抑制の内訳を 1 件詳しく見る
print("\n=== 東京都 千代田区 の全業種 (抑制理由つき) ===")
d = labor_flow("東京都", "千代田区")
for i in sorted(d["industries"], key=lambda x: -x["companies"])[:12]:
    rate = f"{i['headcount_rate_1y']:+.2f}%" if i["headcount_rate_1y"] is not None else "—"
    t1 = f"{i['top1_share_pct']:.1f}%" if i["top1_share_pct"] is not None else "—"
    print(f"  {i['sn_industry'][:14]:<16} n={i['companies']:>4} 増減={i['net_change_1y']:>+7} "
          f"rate={rate:>9} top1={t1:>7}")
    if i["headcount_notice"]:
        print(f"      → {i['headcount_notice']}")

# 集中で抑制された例を全国から探す
print("\n=== 1 社集中で抑制された業種セルの例 ===")
found = 0
for pref, muni in [("東京都", "千代田区"), ("東京都", "港区"), ("広島県", "福山市"),
                   ("大阪府", "東大阪市"), ("北海道", "札幌市白石区"), ("東京都", "")]:
    d = labor_flow(pref, muni)
    for i in d.get("industries", []):
        if i["headcount_notice"] and "1 社が占め" in i["headcount_notice"]:
            print(f"  {pref} {muni} / {i['sn_industry']}: n={i['companies']} "
                  f"top1={i['top1_share_pct']:.1f}% 増減={i['net_change_1y']:+}人")
            print(f"      → {i['headcount_notice']}")
            found += 1
            if found >= 5:
                sys.exit(0)
if not found:
    print("  (見つからず。30社以上かつ1社50%以上のセルが上記地域に無い)")
