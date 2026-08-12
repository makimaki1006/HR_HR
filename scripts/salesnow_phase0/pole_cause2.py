# -*- coding: utf-8 -*-
"""#3 続き: 極近傍は「地域の雇用が減った」のか「記録の対象範囲が変わった」のか。

H5: 持株会社化・分社化などの再編なら、社名に「ホールディングス」等が出る /
    段差が一度きり (3m≒6m≒1y≒2y) / 同一地域に受け皿企業がいる
"""
import re

import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
DELTAS = ["employee_delta_1m", "employee_delta_3m", "employee_delta_6m",
          "employee_delta_1y", "employee_delta_2y"]


def hr(t):
    print("\n" + "=" * 78 + f"\n## {t}\n" + "=" * 78)


df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "sn_industry",
                               "employee_count"] + DELTAS, low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
base = (ec.notna() & (ec > 0) & d.notna() & (d > -100)
        & df["sn_industry"].notna() & (df["sn_industry"].astype(str) != ""))
w = df[base].copy()
w["ec"] = ec[base]
w["d"] = d[base]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
w["past"] = w["ec"] - w["chg"]
m = w[DELTAS].apply(pd.to_numeric, errors="coerce")

hr("H5-a: 極近傍の社名に再編を示す語が出るか")
HOLD = r"ホールディングス|ＨＤ|ホールディング|持株"
for lo in [-95, -90]:
    pole = w["d"] <= lo
    hits = w.loc[pole, "sn_company_name"].astype(str).str.contains(HOLD, regex=True, na=False)
    allhits = w["sn_company_name"].astype(str).str.contains(HOLD, regex=True, na=False)
    print(f"  d <= {lo}: {pole.sum():>4} 社中 {hits.sum():>3} 社 ({hits.mean()*100:>5.1f}%) が持株会社系")
print(f"  全体      : {len(w):,} 社中 {allhits.sum():,} 社 ({allhits.mean()*100:.2f}%)")
print("  → 極近傍で持株会社の比率が跳ね上がるなら、再編 (記録範囲の変更) が主因")

hr("H5-b: 段差が一度きりか (3m ≒ 6m ≒ 1y ≒ 2y)")
pole = w["d"] <= -95
sub = m[pole][["employee_delta_3m", "employee_delta_6m", "employee_delta_1y", "employee_delta_2y"]]
spread = sub.max(axis=1) - sub.min(axis=1)
print(f"  極近傍 {pole.sum()} 社の 3m〜2y の値の幅:")
print("    " + " ".join(f"p{int(q*100)}={spread.quantile(q):.2f}" for q in [.25, .5, .75, .9, 1.0]))
print(f"    幅 < 1 ポイント (ほぼ同値 = 一度きりの段差): {(spread < 1).sum()} 社")
print(f"    幅 < 5 ポイント                          : {(spread < 5).sum()} 社")
print("  → 幅が小さい = 過去のある時点で階段状に落ち、その後動いていない")

hr("H5-c: 同一地域・同一業種に『受け皿』がいるか")
# 極近傍企業と同じ都道府県×業種で、同程度の増加をした企業があるか
pole_rows = w[pole].reindex(w[pole]["chg"].abs().sort_values(ascending=False).index)
print(f"  {'減少企業':<24} {'減少':>7}  同一 都道府県×業種 での最大増加企業")
for i in pole_rows.head(8).index:
    r = w.loc[i]
    peers = w[(w["prefecture"] == r["prefecture"]) & (w["sn_industry"] == r["sn_industry"])
              & (w.index != i)]
    if len(peers) == 0:
        continue
    top = peers.loc[peers["chg"].idxmax()]
    print(f"  {str(r['sn_company_name'])[:22]:<24} {r['chg']:>+7.0f}  "
          f"{str(top['sn_company_name'])[:20]:<22} {top['chg']:>+7.0f}")

hr("除外候補ごとの効果 (基準: d<=-90 を全部除いた姿)")
MIN_COMPANIES, MAX_TOP1 = 30, 50.0


def cells(frame):
    out = {}
    for k, s in frame.groupby(["prefecture", "sn_industry"]):
        n = len(s)
        past, net = s["past"].sum(), s["chg"].sum()
        ta = s["chg"].abs().sum()
        t1 = s["chg"].abs().max() if n else 0
        sh = (t1 / ta * 100) if ta > 0 else None
        if n == 0 or past <= 0:
            out[k] = (None, "NoData")
        elif n < MIN_COMPANIES:
            out[k] = (None, "TooFew")
        elif sh is not None and sh >= MAX_TOP1:
            out[k] = (None, "Conc")
        else:
            out[k] = (net / past * 100, "Show")
    return out


ref = cells(w[w["d"] > -90])
print(f"{'ガード':<22} {'除外':>6} {'表示':>6} {'基準と符号が違う':>18} {'基準と1pt超ズレ':>17}")
for lo, lbl in [(None, "(無し) 現状"), (-99, "d > -99"), (-95, "d > -95"), (-90, "d > -90 (=基準)")]:
    frame = w if lo is None else w[w["d"] > lo]
    cur = cells(frame)
    shown = [k for k, v in cur.items() if v[1] == "Show"]
    sign = sum(1 for k in shown if k in ref and ref[k][0] is not None
               and abs(cur[k][0]) > 0.01 and np.sign(cur[k][0]) != np.sign(ref[k][0]))
    big = sum(1 for k in shown if k in ref and ref[k][0] is not None
              and abs(cur[k][0] - ref[k][0]) > 1.0)
    dropped = len(w) - len(frame)
    print(f"{lbl:<22} {dropped:>6} {len(shown):>6} {sign:>18} {big:>17}")
