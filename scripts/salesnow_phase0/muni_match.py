# -*- coding: utf-8 -*-
"""#5: 市区町村の address マッチ方式を比較する。

現行 : prefecture = P AND address LIKE '%' || strip_county(M) || '%'   (部分一致)
提案 : prefecture = P AND address LIKE P || M || '%'                    (先頭一致・郡込み)

address は 100% が prefecture で始まり、郡も含む (18,536 件) ことを確認済み。
"""
import re

import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MCC = r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv"
PREFS = ["北海道","青森県","岩手県","宮城県","秋田県","山形県","福島県","茨城県","栃木県","群馬県",
         "埼玉県","千葉県","東京都","神奈川県","新潟県","富山県","石川県","福井県","山梨県","長野県",
         "岐阜県","静岡県","愛知県","三重県","滋賀県","京都府","大阪府","兵庫県","奈良県","和歌山県",
         "鳥取県","島根県","岡山県","広島県","山口県","徳島県","香川県","愛媛県","高知県","福岡県",
         "佐賀県","長崎県","熊本県","大分県","宮崎県","鹿児島県","沖縄県"]
KEEP = {"郡山市", "郡上市", "蒲郡市", "上郡町", "大和郡山市", "小郡市"}


def strip_county(s):
    if s in KEEP:
        return s
    m = re.match(r"^.+?郡(.+)$", s)
    return m.group(1) if m else s


df = pd.read_csv(CSV, usecols=["prefecture", "address"], low_memory=False)
df = df[df["address"].notna() & df["prefecture"].notna()]
mc = pd.read_csv(MCC, dtype={"prefcode": str})
mc["prefcode"] = mc["prefcode"].astype(int)
code2pref = {i + 1: p for i, p in enumerate(PREFS)}
mc["pref"] = mc["prefcode"].map(code2pref)

by_pref = {p: g["address"].tolist() for p, g in df.groupby("prefecture")}

rows = []
for _, r in mc.iterrows():
    p, full = r["pref"], r["city_name"]
    if p is None or p not in by_pref:
        continue
    key = strip_county(full)
    addrs = by_pref[p]
    cur = [a for a in addrs if key in a]                 # 現行: 部分一致
    new = [a for a in addrs if a.startswith(p + full)]   # 提案: 先頭一致 (郡込み)
    if not cur and not new:
        continue
    cur_s, new_s = set(cur), set(new)
    rows.append(dict(pref=p, city=full, key=key, cur=len(cur), new=len(new),
                     false_pos=len(cur_s - new_s), missed=len(new_s - cur_s)))

t = pd.DataFrame(rows)
print("=" * 78)
print("## 全市区町村での比較")
print("=" * 78)
print(f"検査した市区町村         : {len(t):,}")
print(f"現行の総ヒット           : {t['cur'].sum():,}")
print(f"提案の総ヒット           : {t['new'].sum():,}")
print(f"現行が拾いすぎ (誤検出)  : {t['false_pos'].sum():,} 社")
print(f"提案が取りこぼす         : {t['missed'].sum():,} 社")
print()
print(f"誤検出がある市区町村: {(t['false_pos'] > 0).sum()} 件")
print(f"  {'都道府県':<8} {'市区町村':<16} {'現行':>6} {'提案':>6} {'誤検出':>7} {'誤検出率':>8}")
for _, r in t[t["false_pos"] > 0].sort_values("false_pos", ascending=False).head(20).iterrows():
    print(f"  {r['pref']:<8} {r['city'][:14]:<16} {r['cur']:>6} {r['new']:>6} "
          f"{r['false_pos']:>7} {r['false_pos']/max(r['cur'],1)*100:>7.1f}%")

print()
print("提案が取りこぼす市区町村 (現行では拾えていたもの):")
miss = t[t["missed"] > 0].sort_values("missed", ascending=False)
if len(miss) == 0:
    print("  なし")
else:
    for _, r in miss.head(15).iterrows():
        print(f"  {r['pref']} {r['city']} (key={r['key']}): 現行 {r['cur']} / 提案 {r['new']} "
              f"→ {r['missed']} 社を取りこぼす")
        # 取りこぼした住所の例
        addrs = by_pref[r["pref"]]
        cur_s = {a for a in addrs if r["key"] in a}
        new_s = {a for a in addrs if a.startswith(r["pref"] + r["city"])}
        for a in list(cur_s - new_s)[:2]:
            print(f"      例: {a[:46]}")

print()
print("=" * 78)
print("## 2026-06-08 の「0 件マッチ」の再検証")
print("=" * 78)
for p, full in [("長崎県", "東彼杵郡東彼杵町"), ("静岡県", "周智郡森町"), ("北海道", "茅部郡森町")]:
    addrs = by_pref.get(p, [])
    n_full = sum(1 for a in addrs if a.startswith(p + full))
    n_strip = sum(1 for a in addrs if strip_county(full) in a)
    print(f"  {p}{full}: 先頭一致(郡込み)={n_full} 社 / 現行の部分一致={n_strip} 社")
