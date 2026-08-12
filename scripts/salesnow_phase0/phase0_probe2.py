# -*- coding: utf-8 -*-
"""フェーズ0 追検証: 外れ値・集計耐性・重心座標カバー・時点ズレの影響。"""
import sys
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MASTER_CITY = r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv"
CENTROIDS = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\media_engine\municipality_centroids.csv"
DELTAS = ["employee_delta_1m", "employee_delta_3m", "employee_delta_6m",
          "employee_delta_1y", "employee_delta_2y"]


def hr(t):
    print("\n" + "=" * 78 + f"\n## {t}\n" + "=" * 78)


df = pd.read_csv(CSV, usecols=["corporate_number", "sn_company_name", "prefecture", "address",
                               "sn_industry", "employee_count", "employee_range",
                               "collated_at"] + DELTAS,
                 dtype={"corporate_number": str}, low_memory=False)
N = len(df)
d1y = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ec = pd.to_numeric(df["employee_count"], errors="coerce")

hr("A. 下限 -100% の意味 (対過去比なら now=0 のはず)")
for c in DELTAS:
    v = pd.to_numeric(df[c], errors="coerce")
    print(f"{c:<22} min={v.min():>10.2f}  max={v.max():>12.2f}  "
          f"= -100 ちょうど: {(v == -100).sum():>6,}  < -100: {(v < -100).sum():>4,}")
print("\n対過去比なら now>=0 より d>=-100 が数学的下限。対現在比なら上限が +100 になるはず。")
print(f"d_1y > 100 の件数: {(d1y > 100).sum():,}  → 上限100の制約が無い = 対現在比ではない")

m100 = (d1y == -100) & ec.notna()
print(f"\nd_1y=-100 かつ employee_count あり: {m100.sum():,} 社")
if m100.sum():
    print(f"  その employee_count の分布: "
          f"{pd.to_numeric(df.loc[m100,'employee_count']).describe()[['min','50%','max']].to_dict()}")
    print(f"  employee_count==0 の割合: {(ec[m100] == 0).mean()*100:.1f}%")

hr("B. 外れ値: 平均が使えるか")
v = d1y.dropna()
print(f"delta_1y  mean={v.mean():.2f}  median={v.median():.2f}  std={v.std():.1f}")
print(f"  上位10値: {sorted(v.values)[-10:]}")
for th in [100, 500, 1000, 10000]:
    print(f"  > {th:>6}% の企業: {(v > th).sum():>5,} 社")
trim = v[(v >= -100) & (v <= 200)]
print(f"\n-100〜200% に収めた場合: n={len(trim):,} ({len(trim)/len(v)*100:.1f}%) "
      f"mean={trim.mean():.2f} median={trim.median():.2f}")
print("→ 単純平均は外れ値に破壊される。増減『人数』へ復元して合計する方式が必要。")

hr("C. 増減人数へ復元 (now*d/(100+d)) した場合の妥当性")
ok = ec.notna() & (ec > 0) & d1y.notna() & (d1y > -100)
chg = ec[ok] * d1y[ok] / (100.0 + d1y[ok])
print(f"復元可能: {ok.sum():,} / {N:,} ({ok.mean()*100:.1f}%)")
print(f"  復元不能の内訳: employee_count 欠損/0 = {(~(ec.notna() & (ec>0))).sum():,}, "
      f"delta_1y 欠損 = {d1y.isna().sum():,}, d=-100 = {(d1y==-100).sum():,}")
print(f"\n増減人数 の分位: " + " ".join(
    f"p{int(q*100)}={chg.quantile(q):.1f}" for q in [0, .01, .25, .5, .75, .99, 1.0]))
print(f"  整数からのズレ最大: {np.abs(chg - np.round(chg)).max():.4f}")
print(f"  全国合計 純増減: {chg.sum():,.0f} 人  (対象 {ok.sum():,} 社)")
past = ec[ok] - chg
print(f"  復元した1年前の総従業員数: {past.sum():,.0f} 人 → 現在 {ec[ok].sum():,.0f} 人 "
      f"({chg.sum()/past.sum()*100:+.2f}%)")
print(f"  1年前が負になった異常: {(past < 0).sum():,} 社")

hr("D. collated_at のズレが集計に与える影響")
ca = pd.to_datetime(df["collated_at"], errors="coerce")
print("『1年前』の基準日は collated_at ごとに異なる。")
print(f"  collated_at の範囲: {ca.min().date()} 〜 {ca.max().date()} (幅 {(ca.max()-ca.min()).days} 日)")
q = ca.dropna().quantile([0, .25, .5, .75, 1.0])
print("  分位: " + " ".join(f"p{int(k*100)}={str(v)[:10]}" for k, v in q.items()))
print(f"\n  2025年内に収集: {(ca < '2026-01-01').sum():,} ({(ca < '2026-01-01').mean()*100:.1f}%)")
print(f"  2026年に収集: {(ca >= '2026-01-01').sum():,} ({(ca >= '2026-01-01').mean()*100:.1f}%)")
print("  → 同一集計内で『2024/11→2025/11 の増減』と『2025/07→2026/07 の増減』が混在する")

# collated_at 群ごとに delta の水準が違うか (時点ズレが結果を歪めるか)
print("\n  収集月別の delta_1y 中央値 (水準が揃っていれば混在の害は小さい):")
tmp = pd.DataFrame({"m": ca.dt.to_period("M"), "d": d1y, "ec": ec})
g = tmp.dropna(subset=["m"]).groupby("m")["d"].agg(["size", "median", "mean"])
for idx, r in g.iterrows():
    if r["size"] < 100:
        continue
    print(f"    {idx}  n={r['size']:>7,.0f}  median={r['median']:>7.2f}  mean={r['mean']:>9.2f}")

hr("E. 市区町村の重心座標カバー (政令市の区)")
cen = pd.read_csv(CENTROIDS)
mc = pd.read_csv(MASTER_CITY, dtype={"citycode": str, "prefcode": str})
cen_names = set(cen["name"])
mc_names = set(mc["city_name"])
ward = mc[mc["city_name"].str.contains("市.*区", regex=True, na=False)]
print(f"master_city 内の政令市の区: {len(ward):,} 件")
print(f"  うち centroids に名前があるもの: {ward['city_name'].isin(cen_names).sum():,}")
print(f"\ncentroids にあって master_city に無い名前: {len(cen_names - mc_names):,}")
print(f"  例: {sorted(cen_names - mc_names)[:15]}")
print(f"\nmaster_city にあって centroids に無い名前: {len(mc_names - cen_names):,}")
print(f"  例: {sorted(mc_names - cen_names)[:15]}")
# 政令市そのもの (福岡市 など) は centroids にあるか
for city in ["福岡市", "名古屋市", "静岡市", "札幌市", "横浜市", "大阪市", "京都市", "浜松市"]:
    print(f"  centroids に '{city}': {city in cen_names}")

hr("F. 重複 corporate_number (二重計上リスク)")
dup = df[df["corporate_number"].duplicated(keep=False)].sort_values("corporate_number")
print(f"重複している行: {len(dup):,} 行 / ユニーク法人 {dup['corporate_number'].nunique():,}")
if len(dup):
    sample = dup.groupby("corporate_number").filter(lambda g: len(g) > 1).head(6)
    print(sample[["corporate_number", "sn_company_name", "employee_count",
                  "employee_delta_1y", "collated_at"]].to_string(index=False))

hr("G. employee_count の欠損と規模の偏り")
print(f"employee_count 欠損: {ec.isna().sum():,} ({ec.isna().mean()*100:.1f}%)")
print(f"employee_count == 0: {(ec == 0).sum():,}")
print("分位: " + " ".join(f"p{int(q*100)}={ec.quantile(q):.0f}" for q in [.05, .25, .5, .75, .95, 1.0]))
print(f"\n全国の従業員数合計: {ec.sum():,.0f} 人")
print("(参考: 日本の雇用者数は約 6,000 万人。この企業データの捕捉範囲を示す)")
print("\n規模帯別の社数:")
print(df["employee_range"].value_counts(dropna=False).to_string())
