# -*- coding: utf-8 -*-
"""adversarial2 の指摘を実データで反証検証する。

(1) delta が -100 近傍の企業は実在するか。極による異常な増減人数が出るか
(2) total_emp が「総従業員」を名乗れるか (フィルタで何 % 落ちているか)
(3) `employee_delta_1y > -100` の追加ガードは実際に何社を除外するか
(4) ドメイン不変条件: 過去人数 >= 0、増減人数の符号整合、合計の一致
"""
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
DELTAS = ["employee_delta_1m", "employee_delta_3m", "employee_delta_6m",
          "employee_delta_1y", "employee_delta_2y"]


def hr(t):
    print("\n" + "=" * 76 + f"\n## {t}\n" + "=" * 76)


df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "sn_industry",
                               "employee_count"] + DELTAS, low_memory=False)
N = len(df)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")

hr("(1) delta が -100 近傍の企業は実在するか (極の危険)")
print("change = ec * d/(100+d)。d → -100 で分母が 0 に近づき増減人数が発散する。")
for lo, hi in [(-100, -99.5), (-99.5, -99), (-99, -95), (-95, -90)]:
    m = (d > lo) & (d <= hi) & ec.notna() & (ec > 0)
    if m.sum() == 0:
        print(f"  {lo} < d <= {hi}: 0 社")
        continue
    chg = ec[m] * d[m] / (100.0 + d[m])
    past = ec[m] - chg
    print(f"  {lo} < d <= {hi}: {m.sum():>4} 社  "
          f"増減人数 min={chg.min():>10.0f}  復元した過去人数 max={past.max():>10.0f}")

worst = (d > -100) & ec.notna() & (ec > 0)
chg_all = ec[worst] * d[worst] / (100.0 + d[worst])
past_all = ec[worst] - chg_all
k = past_all.idxmax()
print(f"\n復元した過去人数が最大の企業:")
print(f"  {df.loc[k,'sn_company_name']} ({df.loc[k,'prefecture']} / {df.loc[k,'sn_industry']})")
print(f"  現在 {ec[k]:.0f} 人, delta={d[k]:.2f}% → 復元した1年前 {past_all[k]:,.0f} 人 "
      f"(増減 {chg_all[k]:+,.0f} 人)")
print(f"\n  復元した過去人数の分位: " +
      " ".join(f"p{int(q*100)}={past_all.quantile(q):,.0f}" for q in [.5, .99, .999, 1.0]))
print(f"  日本の雇用者数 約 6,000 万人 を超える企業: {(past_all > 60_000_000).sum()} 社")
print(f"  過去人数が 100 万人を超える企業: {(past_all > 1_000_000).sum()} 社")
if (past_all > 1_000_000).sum():
    for i in past_all[past_all > 1_000_000].index[:5]:
        print(f"    {df.loc[i,'sn_company_name']}: 現在 {ec[i]:.0f} 人 d={d[i]:.2f}% "
              f"→ 過去 {past_all[i]:,.0f} 人")

hr("(2) total_emp は『総従業員』を名乗れるか")
# SQL の WHERE: employee_count > 0 AND employee_delta_1y IS NOT NULL AND employee_delta_1y > -100
in_agg = ec.notna() & (ec > 0) & d.notna() & (d > -100)
has_emp = ec.notna() & (ec > 0)
print(f"employee_count > 0 の企業           : {has_emp.sum():>8,} 社  "
      f"従業員合計 {ec[has_emp].sum():>12,.0f} 人")
print(f"集計に入る企業 (delta も有効)        : {in_agg.sum():>8,} 社  "
      f"従業員合計 {ec[in_agg].sum():>12,.0f} 人")
excluded = has_emp & ~in_agg
print(f"落ちる企業                          : {excluded.sum():>8,} 社  "
      f"従業員合計 {ec[excluded].sum():>12,.0f} 人")
print(f"\n  → 画面の「総従業員」は実際には従業員の "
      f"{ec[in_agg].sum()/ec[has_emp].sum()*100:.1f}% しか含まない "
      f"({ec[excluded].sum():,.0f} 人が欠落)")
print(f"  → 「{'{cnt}'}社」も同様に {in_agg.sum()/has_emp.sum()*100:.1f}% の企業数")

hr("(3) `employee_delta_1y > -100` の追加ガードは何社を除外するか")
guard_hits = ec.notna() & (ec > 0) & d.notna() & (d <= -100)
print(f"employee_count > 0 かつ delta <= -100 の企業: {guard_hits.sum()} 社")
print("  → 0 なら、このガードは `employee_count > 0` と重複しており除外数を変えない")
d100 = d == -100
print(f"参考: delta == -100 は {d100.sum():,} 社、うち employee_count > 0 は "
      f"{(d100 & (ec > 0)).sum()} 社")

hr("(4) ドメイン不変条件")
chg_r = np.round(chg_all)
past_r = ec[worst] - chg_r
print(f"検査対象: {worst.sum():,} 社")
print(f"  復元した過去人数 < 0 の企業        : {(past_r < 0).sum()}  (0 であるべき)")
print(f"  復元した過去人数 == 0 の企業       : {(past_r == 0).sum()}  (現在>0なら過去0は増加率∞)")
print(f"  増減の符号と delta の符号が不一致  : "
      f"{((np.sign(chg_r) != np.sign(d[worst])) & (chg_r != 0) & (d[worst] != 0)).sum()}  (0 であるべき)")
print(f"  現在 = 過去 + 増減 が崩れる企業    : {((past_r + chg_r) != ec[worst]).sum()}  (0 であるべき)")
# 合計の一致 (地域集計と全国集計)
print(f"\n  全国: 過去 {past_r.sum():,.0f} 人 + 増減 {chg_r.sum():+,.0f} 人 "
      f"= {past_r.sum()+chg_r.sum():,.0f} 人 / 現在合計 {ec[worst].sum():,.0f} 人 "
      f"→ {'一致' if abs(past_r.sum()+chg_r.sum()-ec[worst].sum())<1 else '★不一致'}")

hr("(5) 全期間で -100 近傍を確認")
for c in DELTAS:
    v = pd.to_numeric(df[c], errors="coerce")
    m = (v > -100) & (v <= -99) & ec.notna() & (ec > 0)
    if m.sum():
        ch = ec[m] * v[m] / (100.0 + v[m])
        print(f"  {c:<22} -100<d<=-99: {m.sum():>4} 社  最大の復元過去人数 "
              f"{(ec[m]-ch).max():>12,.0f} 人")
    else:
        print(f"  {c:<22} -100<d<=-99: 0 社")
