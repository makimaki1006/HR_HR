# -*- coding: utf-8 -*-
"""フェーズ0 追検証3: 既存実装 2 箇所の影響を実データで定量化。
  (1) CAST(... AS INTEGER) は SQLite で 0 方向切り捨て。四捨五入ではない
  (2) AVG(employee_delta_1y) は外れ値フィルタ無し
"""
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"


def hr(t):
    print("\n" + "=" * 78 + f"\n## {t}\n" + "=" * 78)


df = pd.read_csv(CSV, usecols=["corporate_number", "prefecture", "address", "sn_industry",
                               "employee_count", "employee_delta_1y", "employee_delta_3m"],
                 dtype={"corporate_number": str}, low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d = pd.to_numeric(df["employee_delta_1y"], errors="coerce")

hr("(1) CAST(x AS INTEGER) の 0方向切り捨てによる誤差")
print("SQLite の CAST(real AS INTEGER) は四捨五入せず 0 方向へ切り捨てる。")
print("実装: SUM(CAST(employee_count * employee_delta_1y / (100.0+employee_delta_1y) AS INTEGER))")
ok = ec.notna() & (ec > 0) & d.notna() & (d > -100)
chg = (ec[ok] * d[ok] / (100.0 + d[ok])).astype(float)
true_int = np.round(chg)          # 真値 (増減人数は整数であることを検証済み)
truncated = np.trunc(chg)         # SQLite CAST の挙動
diff = truncated - true_int
print(f"\n対象: {ok.sum():,} 社")
print(f"  真値(四捨五入)の合計    : {true_int.sum():>12,.0f} 人")
print(f"  CAST(切り捨て)の合計    : {truncated.sum():>12,.0f} 人")
print(f"  差                     : {truncated.sum() - true_int.sum():>12,.0f} 人 "
      f"({(truncated.sum()-true_int.sum())/abs(true_int.sum())*100:+.1f}%)")
print(f"\n  1人ずれた企業: {(diff != 0).sum():,} 社 ({(diff!=0).mean()*100:.1f}%)")
print(f"    うち増加側企業で -1 された: {((diff == -1) & (true_int > 0)).sum():,}")
print(f"    うち減少側企業で +1 された: {((diff == 1) & (true_int < 0)).sum():,}")
print("\n  → 増加は過小、減少は過小に出る。純増減の絶対値が系統的に縮む方向のバイアス。")

hr("(2) AVG(employee_delta_1y) の外れ値汚染 (都道府県 × 業種)")
print("実装: company/fetch.rs:334, jobmap/company_markers.rs:75,93 -- WHERE に値域フィルタ無し")
sub = df[ok].copy()
sub["d"] = d[ok]
sub["ec"] = ec[ok]
sub["chg"] = true_int.values
g = sub.groupby(["prefecture", "sn_industry"])
agg = g.agg(avg=("d", "mean"), med=("d", "median"), n=("d", "size"),
            mx=("d", "max"), sum_ec=("ec", "sum"), sum_chg=("chg", "sum"))
agg = agg[agg["n"] >= 5]
print(f"\n都道府県×業種セル (n>=5): {len(agg):,}")
# 加重平均 (人数ベース) と単純平均の乖離
agg["weighted"] = agg["sum_chg"] / (agg["sum_ec"] - agg["sum_chg"]) * 100
agg["gap"] = agg["avg"] - agg["weighted"]
print(f"\n単純平均 AVG(delta) と 人数加重の増減率 の乖離:")
for q in [.5, .75, .9, .95, .99, 1.0]:
    print(f"  p{int(q*100)}: {agg['gap'].abs().quantile(q):>8.1f} ポイント")
print(f"\n  乖離 10 ポイント超のセル: {(agg['gap'].abs() > 10).sum():,} "
      f"({(agg['gap'].abs()>10).mean()*100:.1f}%)")
print(f"  乖離 50 ポイント超のセル: {(agg['gap'].abs() > 50).sum():,}")
print(f"  セル内に delta>1000% の企業を含むセル: {(agg['mx'] > 1000).sum():,}")
print(f"  セル内に delta>300% の企業を含むセル: {(agg['mx'] > 300).sum():,}")

print("\n  乖離が大きいセル 上位8 (単純平均が実態から離れている例):")
top = agg.reindex(agg["gap"].abs().sort_values(ascending=False).index).head(8)
print(f"  {'都道府県':<8} {'業種':<14} {'n':>5} {'単純平均':>9} {'中央値':>8} {'人数加重':>9} {'最大値':>10}")
for (p, i), r in top.iterrows():
    print(f"  {str(p):<8} {str(i)[:12]:<14} {r['n']:>5.0f} {r['avg']:>9.1f} {r['med']:>8.1f} "
          f"{r['weighted']:>9.1f} {r['mx']:>10.0f}")

hr("(3) 市区町村粒度で見た場合の集計値の安定性")
# 市区町村を address 先頭から粗く抽出 (probe1 と同じ方式の簡易版)
mc = pd.read_csv(r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv",
                 dtype={"prefcode": str})
PREFS = ["北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県", "茨城県", "栃木県",
         "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県", "新潟県", "富山県", "石川県", "福井県",
         "山梨県", "長野県", "岐阜県", "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府",
         "兵庫県", "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県", "徳島県",
         "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県",
         "鹿児島県", "沖縄県"]
pref2code = {p: str(i + 1) for i, p in enumerate(PREFS)}
by_pref = {}
for _, r in mc.iterrows():
    by_pref.setdefault(str(int(r["prefcode"])), []).append(r["city_name"])
for k in by_pref:
    by_pref[k].sort(key=len, reverse=True)


def ex(a, p):
    if not isinstance(a, str) or not a:
        return None
    code = pref2code.get(p)
    if code is None:
        return None
    rest = a[len(p):] if a.startswith(p) else a
    for c in by_pref.get(code, []):
        if rest.startswith(c):
            return c
    return None


sub["city"] = [ex(a, p) for a, p in zip(sub["address"], sub["prefecture"])]
sub2 = sub[sub["city"].notna()]
gc = sub2.groupby(["prefecture", "city"]).agg(
    n=("d", "size"), sum_ec=("ec", "sum"), sum_chg=("chg", "sum"),
    avg=("d", "mean"), med=("d", "median"))
gc["rate"] = gc["sum_chg"] / (gc["sum_ec"] - gc["sum_chg"]) * 100
print(f"市区町村セル: {len(gc):,}")
print("\n企業数の閾値ごとの、人数加重増減率の分布の広がり (外れ値の暴れ具合):")
print(f"  {'閾値':<12} {'市区町村数':>10} {'p5':>8} {'p50':>8} {'p95':>8} {'最大':>10}")
for th in [1, 10, 30, 50, 100]:
    s = gc[gc["n"] >= th]["rate"]
    print(f"  n>={th:<9} {len(s):>10,} {s.quantile(.05):>8.1f} {s.quantile(.5):>8.1f} "
          f"{s.quantile(.95):>8.1f} {s.max():>10.1f}")
print("\n→ 企業数が少ない市区町村ほど、地域の増減率が1社の事情で大きく振れる。")

# 上位1社が地域の純増減に占める割合
def top1_share(x):
    v = x.abs().sort_values(ascending=False)
    tot = v.sum()
    return v.iloc[0] / tot * 100 if tot > 0 else np.nan


sh = sub2.groupby(["prefecture", "city"])["chg"].apply(top1_share)
sh = sh[gc["n"] >= 30]
print(f"\n企業数30社以上の市区町村で、増減量の最大1社が全体に占める割合:")
print("  " + " ".join(f"p{int(q*100)}={sh.quantile(q):.0f}%" for q in [.25, .5, .75, .9]))
print(f"  1社で50%以上を占める市区町村: {(sh >= 50).sum():,} / {sh.notna().sum():,} "
      f"({(sh>=50).mean()*100:.1f}%)")
