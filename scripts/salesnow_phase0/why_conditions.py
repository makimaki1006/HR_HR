# -*- coding: utf-8 -*-
"""「守らないと1社の事情を映すだけの数字になる」の実証。
  (1) 単純平均の +752.7% を1社の寄与に分解する
  (2) 企業数が少ない市区町村で、1社が地域の値をどこまで支配するか
"""
import re
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MCC = r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv"
PREFS = ["北海道","青森県","岩手県","宮城県","秋田県","山形県","福島県","茨城県","栃木県","群馬県",
         "埼玉県","千葉県","東京都","神奈川県","新潟県","富山県","石川県","福井県","山梨県","長野県",
         "岐阜県","静岡県","愛知県","三重県","滋賀県","京都府","大阪府","兵庫県","奈良県","和歌山県",
         "鳥取県","島根県","岡山県","広島県","山口県","徳島県","香川県","愛媛県","高知県","福岡県",
         "佐賀県","長崎県","熊本県","大分県","宮崎県","鹿児島県","沖縄県"]
p2c = {p: str(i + 1) for i, p in enumerate(PREFS)}


def hr(t):
    print("\n" + "=" * 76 + f"\n## {t}\n" + "=" * 76)


df = pd.read_csv(CSV, usecols=["sn_company_name", "prefecture", "address", "sn_industry",
                               "employee_count", "employee_delta_1y"], low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ok = ec.notna() & (ec > 0) & d.notna() & (d > -100)
w = df[ok].copy()
w["ec"] = ec[ok]
w["d"] = d[ok]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))   # 増減人数 (真値)
w["past"] = w["ec"] - w["chg"]

hr("(1) 東京都 × 人材・アウトソーシング の +752.7% を分解する")
cell = w[(w["prefecture"] == "東京都") & (w["sn_industry"] == "人材・アウトソーシング")]
n = len(cell)
avg_all = cell["d"].mean()
top = cell.loc[cell["d"].idxmax()]
rest = cell.drop(top.name)
print(f"対象: {n} 社   単純平均 AVG(delta) = {avg_all:+.1f}%")
print(f"\n最大値の1社: {top['sn_company_name']}")
print(f"  employee_count={top['ec']:.0f}  delta_1y={top['d']:,.0f}%  "
      f"→ 復元すると 1年前 {top['past']:.0f} 人 → 現在 {top['ec']:.0f} 人 ({top['chg']:+.0f} 人)")
print(f"\n平均への寄与の分解:")
print(f"  この1社が押し上げた分   : {top['d']/n:>10.1f} ポイント "
      f"({top['d']/n/avg_all*100:.1f}%)")
print(f"  残り {n-1} 社の平均      : {rest['d'].mean():>10.1f} ポイント")
print(f"  合計                   : {top['d']/n + rest['d'].mean()*(n-1)/n:>10.1f} ポイント")
print(f"\n→ 表示値 {avg_all:+.1f}% のうち {top['d']/n/avg_all*100:.1f}% が1社由来。")
print(f"  この1社を除くだけで平均は {rest['d'].mean():+.2f}% になる。")

print(f"\n同じ696社を『人数』で見た場合 (人数加重):")
tot_past, tot_chg = cell["past"].sum(), cell["chg"].sum()
print(f"  1年前 {tot_past:,.0f} 人 → 現在 {cell['ec'].sum():,.0f} 人  "
      f"純増 {tot_chg:+,.0f} 人 = {tot_chg/tot_past*100:+.2f}%")
print(f"  うちこの1社の寄与: {top['chg']:+,.0f} 人 / {tot_chg:+,.0f} 人 "
      f"= {top['chg']/tot_chg*100:.1f}%")
print("\n→ 人数で見れば『大きいが支配的ではない1社』。率の平均だと支配する。")
print("   理由: 増減率は分母(過去の人数)が小さいほど爆発する。")
print("   1人→2人 は +100%、1,000人→1,001人 は +0.1%。同じ『1人増』で1,000倍の差。")
print("   単純平均は全社を1票ずつ数えるので、分母1人の会社が696社分の重みを持ちうる。")

hr("(2) 企業数が少ない市区町村では、外れ値を除いても1社が支配する")
mc = pd.read_csv(MCC, dtype={"prefcode": str})
mc["prefcode"] = mc["prefcode"].astype(int).astype(str)
bp = {}
for _, r in mc.iterrows():
    bp.setdefault(r["prefcode"], []).append(r["city_name"])
for k in bp:
    bp[k].sort(key=len, reverse=True)


def ex(a, p):
    c = p2c.get(p)
    if c is None or not isinstance(a, str):
        return None
    rest = a[len(p):] if a.startswith(p) else a
    for x in bp.get(c, []):
        if rest.startswith(x):
            return x
    return None


w["city"] = [ex(a, p) for a, p in zip(w["address"], w["prefecture"])]
ww = w[w["city"].notna()].copy()
# 外れ値 (±300% 超) を既に除外した上で評価する = 条件1を守った後でも残る問題
ww = ww[ww["d"].abs() <= 300]

g = ww.groupby(["prefecture", "city"])
summary = g.agg(n=("d", "size"), past=("past", "sum"), chg=("chg", "sum"))
summary["rate"] = summary["chg"] / summary["past"] * 100
# 増減量の最大1社が地域の純増減の絶対値に占める割合
top_share = g["chg"].apply(lambda s: s.abs().max() / s.abs().sum() * 100 if s.abs().sum() > 0 else np.nan)
summary["top1"] = top_share

print("※ ここでは ±300% 超の外れ値を既に除外済み。条件1を守った後でも残る問題を見る。\n")
print(f"{'企業数':<14} {'市区町村数':>10} {'最大1社の占有率 中央値':>22} {'1社で50%超':>12} {'1社で80%超':>12}")
for lo, hi, lab in [(1, 5, "1-4社"), (5, 10, "5-9社"), (10, 30, "10-29社"),
                    (30, 100, "30-99社"), (100, 10**9, "100社以上")]:
    s = summary[(summary["n"] >= lo) & (summary["n"] < hi)]
    if len(s) == 0:
        continue
    t = s["top1"].dropna()
    print(f"{lab:<14} {len(s):>10,} {t.median():>21.0f}% "
          f"{(t>=50).mean()*100:>11.0f}% {(t>=80).mean()*100:>11.0f}%")

print("\n→ 30社未満では、地域の純増減の過半を1社が握る市区町村が多数派に近づく。")
print("  外れ値を除いても消えない。単に母数が足りない。")

hr("(3) 具体例: 30社未満の市区町村で何が起きるか")
small = summary[(summary["n"] < 30) & (summary["n"] >= 5) & (summary["top1"] >= 80)]
small = small.reindex(small["rate"].abs().sort_values(ascending=False).index)
print(f"該当: {len(small)} 市区町村 (5-29社 かつ 1社が80%以上を占める)\n")
print(f"{'都道府県':<8} {'市区町村':<14} {'社数':>5} {'地域の増減率':>12} {'最大1社':>8}")
for (p, c), r in small.head(6).iterrows():
    print(f"{p:<8} {c:<14} {r['n']:>5.0f} {r['rate']:>11.1f}% {r['top1']:>7.0f}%")

if len(small):
    (p, c) = small.index[0]
    cell2 = ww[(ww["prefecture"] == p) & (ww["city"] == c)].copy()
    cell2 = cell2.reindex(cell2["chg"].abs().sort_values(ascending=False).index)
    print(f"\n--- {p}{c} の内訳 ({len(cell2)} 社) ---")
    print(f"  地域の増減率 {summary.loc[(p,c),'rate']:+.1f}% "
          f"(1年前 {summary.loc[(p,c),'past']:,.0f} 人 → 純増 {summary.loc[(p,c),'chg']:+,.0f} 人)")
    print(f"\n  {'企業名':<26} {'業種':<12} {'現在':>6} {'増減':>6} {'率':>8}")
    for _, r in cell2.head(6).iterrows():
        print(f"  {str(r['sn_company_name'])[:24]:<26} {str(r['sn_industry'])[:10]:<12} "
              f"{r['ec']:>6.0f} {r['chg']:>+6.0f} {r['d']:>+7.1f}%")
    print(f"\n  → 地図はこの市区町村を1色で塗る。見る人は『この地域が伸びている』と読む。")
    print(f"    実際に起きたのは1社の増員であって、地域の傾向ではない。")
