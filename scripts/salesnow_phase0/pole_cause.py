# -*- coding: utf-8 -*-
"""#3 の原因調査: 極近傍 (delta ≈ -100) は「実際の縮小」か「データ不整合」か。

症状を隠す前に原因を分ける。検証する仮説:
  H1: collated_at が古い (1 年前の基準がずれている)
  H2: employee_count 側が壊れている (employee_range と矛盾する)
  H3: 期間間で矛盾している (1y は -99% なのに 6m/3m は 0% 等)
  H4: group_employee_count と矛盾する (単体は 13 人だがグループは数千人)
"""
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
DELTAS = ["employee_delta_1m", "employee_delta_3m", "employee_delta_6m",
          "employee_delta_1y", "employee_delta_2y"]


def hr(t):
    print("\n" + "=" * 78 + f"\n## {t}\n" + "=" * 78)


df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "sn_industry", "employee_count",
                               "employee_range", "group_employee_count", "collated_at",
                               "established_date", "listing_category"] + DELTAS,
                 low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
base = ec.notna() & (ec > 0) & d.notna() & (d > -100)
w = df[base].copy()
w["ec"] = ec[base]
w["d"] = d[base]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
w["past"] = w["ec"] - w["chg"]
w["ratio"] = w["past"] / w["ec"]

POLE = w["d"] <= -95          # 問題の 94 社
NORMAL = w["d"] > -95
print(f"検査対象: 全 {len(w):,} 社 / 極近傍 (d<=-95) {POLE.sum()} 社")

hr("H1: collated_at が古いか")
ca = pd.to_datetime(w["collated_at"], errors="coerce")
print(f"{'群':<18} {'件数':>7} {'収集日 中央値':>14} {'2025-11-10 の割合':>18}")
for lbl, m in [("極近傍 (d<=-95)", POLE), ("通常 (d>-95)", NORMAL)]:
    sub = ca[m].dropna()
    r = (sub.dt.date.astype(str) == "2025-11-10").mean() * 100
    print(f"{lbl:<18} {len(sub):>7,} {str(sub.median())[:10]:>14} {r:>17.1f}%")
print("\n→ 収集日の分布が同じなら H1 (基準日ずれ) は原因ではない")

hr("H2: employee_count が employee_range と矛盾しないか")
# employee_range のラベルから下限人数を取る
RANGE_MIN = {"0: 5人未満": 0, "1: 5人以上~10人未満": 5, "2: 10人以上~20人未満": 10,
             "3: 20人以上~50人未満": 20, "4: 50人以上~300人未満": 50,
             "5: 300人以上~1,000人未満": 300, "6: 1,000人以上~3,000人未満": 1000,
             "7: 3,000人以上~10,000人未満": 3000, "8: 10,000人以上": 10000}
RANGE_MAX = {"0: 5人未満": 5, "1: 5人以上~10人未満": 10, "2: 10人以上~20人未満": 20,
             "3: 20人以上~50人未満": 50, "4: 50人以上~300人未満": 300,
             "5: 300人以上~1,000人未満": 1000, "6: 1,000人以上~3,000人未満": 3000,
             "7: 3,000人以上~10,000人未満": 10000, "8: 10,000人以上": 10**9}
w["rmin"] = w["employee_range"].map(RANGE_MIN)
w["rmax"] = w["employee_range"].map(RANGE_MAX)
has_range = w["rmin"].notna()
# employee_count が range の外に出ている = どちらかが古い
out = has_range & ((w["ec"] < w["rmin"]) | (w["ec"] >= w["rmax"]))
print(f"{'群':<18} {'range あり':>10} {'ec が range 外':>14} {'割合':>8}")
for lbl, m in [("極近傍 (d<=-95)", POLE), ("通常 (d>-95)", NORMAL)]:
    n = (has_range & m).sum()
    o = (out & m).sum()
    print(f"{lbl:<18} {n:>10,} {o:>14,} {o/max(n,1)*100:>7.1f}%")

# range が示す規模と past のどちらが近いか
print("\n極近傍のうち range が employee_count より『過去人数』に近い企業:")
pole_r = w[POLE & has_range].copy()
close_to_past = ((pole_r["past"] >= pole_r["rmin"]) & (pole_r["past"] < pole_r["rmax"])).sum()
close_to_ec = ((pole_r["ec"] >= pole_r["rmin"]) & (pole_r["ec"] < pole_r["rmax"])).sum()
print(f"  range が現在の employee_count と整合: {close_to_ec} 社")
print(f"  range が復元した過去人数と整合      : {close_to_past} 社")
print("  → 後者が多ければ employee_count 側が古い (= delta ではなく人数が壊れている)")
print("\n  例 (極近傍、range 付き):")
print(f"  {'企業名':<26} {'現在':>6} {'過去':>8} {'employee_range':<24}")
for i in pole_r.reindex(pole_r["chg"].abs().sort_values(ascending=False).index).head(8).index:
    r = w.loc[i]
    print(f"  {str(r['sn_company_name'])[:24]:<26} {r['ec']:>6.0f} {r['past']:>8.0f} "
          f"{str(r['employee_range'])[:22]:<24}")

hr("H3: 期間間の整合性")
m = w[DELTAS].apply(pd.to_numeric, errors="coerce")
print("極近傍 (d_1y<=-95) の企業で、他期間はどうなっているか:")
print(f"  {'企業名':<24} {'1m':>9} {'3m':>9} {'6m':>9} {'1y':>9} {'2y':>9}")
pole_idx = w[POLE].reindex(w[POLE]["chg"].abs().sort_values(ascending=False).index).head(10).index
for i in pole_idx:
    vals = " ".join(f"{m.loc[i,c]:>9.2f}" if pd.notna(m.loc[i, c]) else f"{'-':>9}"
                    for c in DELTAS)
    print(f"  {str(w.loc[i,'sn_company_name'])[:22]:<24}{vals}")
# 6m が -50% より浅いのに 1y が -95% 未満 = 直近半年では動いていないのに1年で激減
inconsist = POLE & m["employee_delta_6m"].notna() & (m["employee_delta_6m"] > -50)
print(f"\n  1y <= -95% なのに 6m > -50% の企業: {inconsist.sum()} / {POLE.sum()} 社")
print("  (半年では減っていないのに 1 年では激減 = 6〜12 か月前に起きた or データ不整合)")
deep6 = POLE & m["employee_delta_6m"].notna() & (m["employee_delta_6m"] <= -90)
print(f"  1y <= -95% かつ 6m <= -90% の企業 : {deep6.sum()} 社 (直近半年で実際に激減)")

hr("H4: group_employee_count との矛盾")
g = pd.to_numeric(w["group_employee_count"], errors="coerce")
print(f"{'群':<18} {'group あり':>10} {'group >= 過去人数':>18} {'group < 現在人数':>16}")
for lbl, mk in [("極近傍 (d<=-95)", POLE), ("通常 (d>-95)", NORMAL)]:
    n = (g.notna() & (g > 0) & mk).sum()
    ge = (g.notna() & (g > 0) & mk & (g >= w["past"])).sum()
    lt = (g.notna() & (g > 0) & mk & (g < w["ec"])).sum()
    print(f"{lbl:<18} {n:>10,} {ge:>18,} {lt:>16,}")
print("\n  → 極近傍で group が過去人数以上なら、過去人数の方が実態に近い可能性がある")

hr("結論の材料")
print(f"極近傍 94 社の内訳:")
print(f"  employee_count が employee_range の外: {(out & POLE).sum()} 社")
print(f"  6m が浅い (直近半年は動いていない)   : {inconsist.sum()} 社")
print(f"  6m も深い (直近半年で実際に激減)     : {deep6.sum()} 社")
print(f"  上場企業                             : {(w[POLE]['listing_category'].astype(str).str.contains('上場', na=False) & ~w[POLE]['listing_category'].astype(str).str.contains('未上場', na=False)).sum()} 社")
