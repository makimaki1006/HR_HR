# -*- coding: utf-8 -*-
"""極近傍ガードの閾値を実データから決める。勘で置かない。

候補の指標:
  A. delta の下限     : d > -X
  B. 復元倍率         : past / ec <= K  (1年前が現在の K 倍を超えたら捨てる)
  C. 丸め由来の不確かさ: uncertainty = ec*100/(100+d)^2 * 0.005 <= U 人
"""
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MIN_COMPANIES, MAX_TOP1 = 30, 50.0

df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "sn_industry",
                               "employee_count", "employee_delta_1y"], low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ok = (ec.notna() & (ec > 0) & d.notna() & (d > -100)
      & df["sn_industry"].notna() & (df["sn_industry"].astype(str) != ""))
w = df[ok].copy()
w["ec"] = ec[ok]
w["d"] = d[ok]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
w["past"] = w["ec"] - w["chg"]
w["ratio"] = w["past"] / w["ec"]
w["unc"] = w["ec"] * 100.0 / (100.0 + w["d"]) ** 2 * 0.005

print("=" * 78)
print("## 復元倍率 past/ec の分布 (1年前が現在の何倍だったか)")
print("=" * 78)
q = w["ratio"].quantile([.5, .9, .99, .999, .9999, 1.0])
print("  " + "  ".join(f"p{k*100:g}={v:.2f}" for k, v in q.items()))
for k in [2, 3, 5, 10, 20, 50, 100]:
    n = (w["ratio"] > k).sum()
    print(f"  past/ec > {k:>3} 倍: {n:>5} 社 ({n/len(w)*100:.3f}%)")


def evaluate(mask_keep, label):
    """ガード適用後に、表示セル数・符号反転・ゲート変化がどうなるか"""
    kept = w[mask_keep]
    dropped = (~mask_keep).sum()

    def stats(sub):
        n = len(sub)
        past, net = sub["past"].sum(), sub["chg"].sum()
        ta = sub["chg"].abs().sum()
        t1 = sub["chg"].abs().max() if n else 0
        sh = (t1 / ta * 100) if ta > 0 else None
        if n == 0 or past <= 0:
            return None, "NoData", None
        if n < MIN_COMPANIES:
            return None, "TooFew", sh
        if sh is not None and sh >= MAX_TOP1:
            return None, "Concentrated", sh
        return net / past * 100, "Show", sh

    base = {k: stats(s) for k, s in w.groupby(["prefecture", "sn_industry"])}
    new = {k: stats(s) for k, s in kept.groupby(["prefecture", "sn_industry"])}
    shown_base = sum(1 for v in base.values() if v[1] == "Show")
    shown_new = sum(1 for v in new.values() if v[1] == "Show")
    # 残った表示セルが、さらに極近傍を除いても符号を保つか
    flips = 0
    for k, v in new.items():
        if v[1] != "Show":
            continue
        sub = kept[(kept["prefecture"] == k[0]) & (kept["sn_industry"] == k[1])]
        core = sub[sub["ratio"] <= 1.5]
        if len(core) == 0:
            continue
        r2 = core["chg"].sum() / core["past"].sum() * 100 if core["past"].sum() > 0 else None
        if r2 is not None and v[0] is not None and abs(v[0]) > 0.01 \
                and np.sign(v[0]) != np.sign(r2):
            flips += 1
    print(f"{label:<34} 除外 {dropped:>4} 社  表示 {shown_base}→{shown_new}  残る符号反転 {flips}")


print()
print("=" * 78)
print("## ガード候補ごとの効果")
print("=" * 78)
print(f"{'ガード':<34} {'除外社数':>8}  {'表示セル':>12}  {'残る符号反転':>12}")
evaluate(pd.Series(True, index=w.index), "(ガード無し) 現状")
for x in [95, 90, 80, 50]:
    evaluate(w["d"] > -x, f"A. delta > -{x}")
for k in [3, 5, 10, 20]:
    evaluate(w["ratio"] <= k, f"B. past/ec <= {k} 倍")
for u in [1, 5, 10]:
    evaluate(w["unc"] <= u, f"C. 丸め不確かさ <= ±{u} 人")

print()
print("=" * 78)
print("## 候補 B の詳細: past/ec > K で落ちる企業は妥当な除外先か")
print("=" * 78)
for k in [5, 10]:
    dropped = w[w["ratio"] > k]
    print(f"\npast/ec > {k} 倍 で落ちる {len(dropped)} 社:")
    print(f"  employee_count の分布: " +
          " ".join(f"p{int(qq*100)}={dropped['ec'].quantile(qq):.0f}" for qq in [.5, .9, 1.0]))
    print(f"  従業員 1 人の企業: {(dropped['ec']==1).sum()} 社 "
          f"({(dropped['ec']==1).mean()*100:.0f}%)")
    print(f"  従業員 10 人未満 : {(dropped['ec']<10).sum()} 社 "
          f"({(dropped['ec']<10).mean()*100:.0f}%)")
    print("  例:")
    for i in dropped.reindex(dropped["chg"].abs().sort_values(ascending=False).index).head(5).index:
        r = w.loc[i]
        print(f"    {str(r['sn_company_name'])[:26]:<28} ec={r['ec']:>5.0f} d={r['d']:>8.2f} "
              f"→ 1年前 {r['past']:>8.0f} 人 ({r['ratio']:>7.1f} 倍)")
