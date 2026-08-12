# -*- coding: utf-8 -*-
"""表示ゲートの閾値を実測で決める。勘で閾値を置かない。
  ゲート1: 企業数 n >= K
  ゲート2: 最大1社の占有率 < S%
  ゲート3: 過去従業員数 >= B  (必要かどうかを実測で判断する)
"""
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


df = pd.read_csv(CSV, usecols=["sn_company_name","prefecture","address","sn_industry",
                               "employee_count","employee_delta_1y"], low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
ok = ec.notna() & (ec > 0) & d.notna() & (d > -100)
w = df[ok].copy()
w["ec"] = ec[ok]; w["d"] = d[ok]
w["chg"] = np.round(w["ec"] * w["d"] / (100.0 + w["d"]))
w["past"] = w["ec"] - w["chg"]

mc = pd.read_csv(MCC, dtype={"prefcode": str}); mc["prefcode"] = mc["prefcode"].astype(int).astype(str)
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

g = ww.groupby(["prefecture", "city"])
s = g.agg(n=("d","size"), past=("past","sum"), cur=("ec","sum"), net=("chg","sum"))
s["abs_chg"] = g["chg"].apply(lambda x: x.abs().sum())
s["top1"] = g["chg"].apply(lambda x: x.abs().max())
s["top1_share"] = s["top1"] / s["abs_chg"] * 100
s["rate"] = s["net"] / s["past"] * 100

hr("ゲートを段階適用したときの、地域増減率の安定性")
print(f"{'適用したゲート':<38} {'残る市区町村':>10} {'p1':>7} {'p50':>7} {'p99':>7} {'最大絶対値':>10}")


def show(label, sel):
    r = s.loc[sel, "rate"]
    print(f"{label:<38} {len(r):>10,} {r.quantile(.01):>6.1f}% {r.quantile(.5):>6.1f}% "
          f"{r.quantile(.99):>6.1f}% {r.abs().max():>9.1f}%")


show("(ゲート無し)", s.index)
show("n>=30", s["n"] >= 30)
show("n>=30 かつ 最大1社<50%", (s["n"] >= 30) & (s["top1_share"] < 50))
show("n>=30 かつ 最大1社<40%", (s["n"] >= 30) & (s["top1_share"] < 40))
show("n>=50 かつ 最大1社<50%", (s["n"] >= 50) & (s["top1_share"] < 50))

hr("ゲート3 (過去従業員数の下限) は必要か")
base = s[(s["n"] >= 30) & (s["top1_share"] < 50)]
print(f"n>=30 かつ 最大1社<50% を満たす {len(base):,} 市区町村の past_employees:")
print("  " + " ".join(f"p{int(q*100)}={base['past'].quantile(q):,.0f}" for q in [0,.01,.05,.5,1.0]))
print(f"\n  past < 500 人 の市区町村: {(base['past']<500).sum()}")
print(f"  past < 1000 人 の市区町村: {(base['past']<1000).sum()}")
if (base["past"] < 1000).sum():
    small = base[base["past"] < 1000]
    print(f"  その増減率: {' '.join(f'{v:+.1f}%' for v in small['rate'].head(8))}")
print("\n→ n>=30 を課すと past も自然に大きくなる。独立したゲート3の必要性を判断する材料。")

hr("ゲートで消える市区町村と、残る企業のカバー率")
for lab, sel in [("n>=30", s["n"] >= 30),
                 ("n>=30 & top1<50%", (s["n"] >= 30) & (s["top1_share"] < 50))]:
    kept = s.loc[sel]
    print(f"{lab:<22} 市区町村 {len(kept):>5,}/{len(s):,} ({len(kept)/len(s)*100:>4.1f}%)  "
          f"企業 {kept['n'].sum():>7,.0f}/{s['n'].sum():,.0f} ({kept['n'].sum()/s['n'].sum()*100:.1f}%)  "
          f"従業員 {kept['cur'].sum()/s['cur'].sum()*100:.1f}%")
print("\n→ 市区町村の数では多くが落ちるが、企業・従業員のカバー率は保たれる。")
print("  落ちるのは元々ほとんど企業が居ない地域である。")

hr("テスト用の実値: 山梨県南都留郡道志村 (全 9 社)")
v = ww[(ww["prefecture"] == "山梨県") & (ww["city"] == "南都留郡道志村")].copy()
v = v.reindex(v["chg"].abs().sort_values(ascending=False).index)
print(f"{'企業名':<26} {'現在':>6} {'delta%':>10} {'増減':>6} {'過去':>6}")
for _, r in v.iterrows():
    print(f"{str(r['sn_company_name'])[:24]:<26} {r['ec']:>6.0f} {r['d']:>10.2f} "
          f"{r['chg']:>+6.0f} {r['past']:>6.0f}")
print(f"\n合計: {len(v)} 社  現在 {v['ec'].sum():.0f} 人  過去 {v['past'].sum():.0f} 人  "
      f"純増 {v['chg'].sum():+.0f} 人")
print(f"  人数加重増減率 = {v['chg'].sum()/v['past'].sum()*100:+.2f}%")
print(f"  Σ|増減| = {v['chg'].abs().sum():.0f}  最大1社 = {v['chg'].abs().max():.0f}  "
      f"占有率 = {v['chg'].abs().max()/v['chg'].abs().sum()*100:.1f}%")
print(f"  単純平均 AVG(delta) = {v['d'].mean():+.2f}%  ← 参考")

hr("テスト用の実値: 東京都 × 人材・アウトソーシング (集約値)")
c = ww[(ww["prefecture"] == "東京都") & (ww["sn_industry"] == "人材・アウトソーシング")]
print(f"社数 {len(c)}  現在 {c['ec'].sum():.0f} 人  過去 {c['past'].sum():.0f} 人  "
      f"純増 {c['chg'].sum():+.0f} 人")
print(f"  人数加重増減率 = {c['chg'].sum()/c['past'].sum()*100:+.2f}%")
print(f"  Σ|増減| = {c['chg'].abs().sum():.0f}  最大1社 = {c['chg'].abs().max():.0f}  "
      f"占有率 = {c['chg'].abs().max()/c['chg'].abs().sum()*100:.1f}%")
print(f"  単純平均 AVG(delta) = {c['d'].mean():+.2f}%  ← 現行実装が返す値")
