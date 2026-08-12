# -*- coding: utf-8 -*-
"""フェーズ0 追検証4: フェーズ1 (重心座標での可視化) が成立するかの最終確認。
centroids は「札幌市中央区」形式と「白石区」形式が混在し、都道府県列が無い。
実際に企業を配置できる率と、曖昧さで配置できない企業数を数える。
"""
import re
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
CEN = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\media_engine\municipality_centroids.csv"
MCC = r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv"

PREFS = ["北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県", "茨城県", "栃木県",
         "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県", "新潟県", "富山県", "石川県", "福井県",
         "山梨県", "長野県", "岐阜県", "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府",
         "兵庫県", "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県", "徳島県",
         "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県",
         "鹿児島県", "沖縄県"]
pref2code = {p: str(i + 1) for i, p in enumerate(PREFS)}
KEEP = {"郡山市", "郡上市", "蒲郡市", "上郡町", "大和郡山市", "小郡市"}


def strip_county(s):
    if s in KEEP:
        return s
    m = re.match(r"^.+?郡(.+)$", s)
    return m.group(1) if m else s


cen = pd.read_csv(CEN)
mc = pd.read_csv(MCC, dtype={"prefcode": str})
mc["prefcode"] = mc["prefcode"].astype(int).astype(str)

# 市区町村名 -> それが存在しうる都道府県コードの集合 (曖昧さの判定用)
name_prefs = {}
for _, r in mc.iterrows():
    for key in {r["city_name"], strip_county(r["city_name"])}:
        name_prefs.setdefault(key, set()).add(r["prefcode"])
    # 政令市の区の「素の区名」(札幌市白石区 -> 白石区)
    m = re.match(r"^.+市(.+区)$", r["city_name"])
    if m:
        name_prefs.setdefault(m.group(1), set()).add(r["prefcode"])

cen_by_name = {r["name"]: (r["lat"], r["lon"], r["level"]) for _, r in cen.iterrows()}

by_pref = {}
for _, r in mc.iterrows():
    by_pref.setdefault(r["prefcode"], []).append(r["city_name"])
for k in by_pref:
    by_pref[k].sort(key=len, reverse=True)


def extract_city(a, p):
    code = pref2code.get(p)
    if code is None or not isinstance(a, str):
        return None
    rest = a[len(p):] if a.startswith(p) else a
    for c in by_pref.get(code, []):
        if rest.startswith(c):
            return c
    return None


def resolve_centroid(city, prefcode):
    """master_city 名 -> centroids の座標。戻り値 (状態, 使ったキー)"""
    cands = [city, strip_county(city)]
    m = re.match(r"^.+市(.+区)$", city)
    if m:
        cands.append(m.group(1))
    for key in cands:
        if key in cen_by_name:
            # centroids に都道府県列が無いため、同名が複数県にある場合は特定不能
            if len(name_prefs.get(key, set())) > 1:
                return ("曖昧", key)
            return ("一意", key)
    return ("欠落", None)


df = pd.read_csv(CSV, usecols=["prefecture", "address", "employee_count", "employee_delta_1y"],
                 low_memory=False)
ec = pd.to_numeric(df["employee_count"], errors="coerce")
d1y = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
df["_usable"] = ec.notna() & (ec > 0) & d1y.notna() & (d1y > -100)
df["_city"] = [extract_city(a, p) for a, p in zip(df["address"], df["prefecture"])]
df["_pc"] = df["prefecture"].map(pref2code)

sub = df[df["_city"].notna() & df["_usable"]].copy()
res = [resolve_centroid(c, pc) for c, pc in zip(sub["_city"], sub["_pc"])]
sub["_state"] = [r[0] for r in res]

print("=" * 78)
print("## フェーズ1 成立性: 企業を市区町村の重心へ配置できるか")
print("=" * 78)
N = len(df)
print(f"\n全 {N:,} 社のうち")
print(f"  人員推移が使える (employee_count>0 & delta_1y 有効): {df['_usable'].sum():,} "
      f"({df['_usable'].mean()*100:.1f}%)")
print(f"  かつ住所から市区町村を特定できた            : {len(sub):,} "
      f"({len(sub)/N*100:.1f}%)")
print("\n重心座標への配置結果 (上記 {:,} 社):".format(len(sub)))
vc = sub["_state"].value_counts()
for k in ["一意", "曖昧", "欠落"]:
    n = vc.get(k, 0)
    print(f"  {k:<4}: {n:>8,} 社 ({n/len(sub)*100:>5.1f}%)")
print(f"\n→ 地図に正しく置ける企業: {vc.get('一意',0):,} / {N:,} = {vc.get('一意',0)/N*100:.1f}%")

print("\n--- 「曖昧」の内訳 (centroids に都道府県列が無く同名が複数県に存在) ---")
amb = sub[sub["_state"] == "曖昧"].groupby(["prefecture", "_city"]).size().sort_values(ascending=False)
print(f"  該当市区町村: {len(amb)} 件")
for (p, c), n in amb.head(12).items():
    print(f"    {p:<6} {c:<14} {n:>6,} 社")

print("\n--- 「欠落」の内訳 (centroids にそもそも無い) ---")
miss = sub[sub["_state"] == "欠落"].groupby(["prefecture", "_city"]).size().sort_values(ascending=False)
print(f"  該当市区町村: {len(miss)} 件")
for (p, c), n in miss.head(12).items():
    print(f"    {p:<6} {c:<14} {n:>6,} 社")

print("\n--- 参考: 区を政令市本体に丸めた場合の救済量 ---")
def rollup(city):
    m = re.match(r"^(.+市).+区$", city)
    return m.group(1) if m else city
bad = sub[sub["_state"].isin(["曖昧", "欠落"])].copy()
bad["_rolled"] = bad["_city"].map(rollup)
saved = bad[bad["_rolled"] != bad["_city"]]
print(f"  区 → 市 に丸めれば置ける企業: {len(saved):,} 社 "
      f"(粒度は市単位に粗くなる)")
print(f"  それでも置けない企業: {len(bad) - len(saved):,} 社")
