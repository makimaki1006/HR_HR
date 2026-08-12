# -*- coding: utf-8 -*-
"""フェーズ0: v2_salesnow_companies の実データ実査。
仕様書・カラム名を信じず、実値の分布で単位を確定させる。
"""
import sys
import numpy as np
import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
MASTER_CITY = r"C:\Users\fuji1\AppData\Local\Temp\HR_HR_salesnow_map\src\geo\master_city.csv"
CENTROIDS = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\media_engine\municipality_centroids.csv"

DELTAS = ["employee_delta_1m", "employee_delta_3m", "employee_delta_6m",
          "employee_delta_1y", "employee_delta_2y"]

USECOLS = ["corporate_number", "sn_company_name", "prefecture", "address", "postal_code",
           "sn_industry", "sn_industry2", "employee_count", "employee_range",
           "group_employee_count", "capital_stock", "sales_amount",
           "collated_at", "established_date"] + DELTAS


def hr(title):
    print("\n" + "=" * 78)
    print(f"## {title}")
    print("=" * 78)


print("読込中...", file=sys.stderr)
df = pd.read_csv(CSV, usecols=USECOLS, dtype={"corporate_number": str, "postal_code": str},
                 low_memory=False)
N = len(df)

hr("1. 全体規模")
print(f"総行数: {N:,}")
print(f"corporate_number ユニーク: {df['corporate_number'].nunique():,}")
print(f"重複行(corporate_number): {N - df['corporate_number'].nunique():,}")

# ---------------------------------------------------------------- 2. delta の実分布
hr("2. employee_delta_* の実分布 (単位判定の一次資料)")
for c in DELTAS:
    s = pd.to_numeric(df[c], errors="coerce")
    nn = s.notna().sum()
    print(f"\n--- {c} ---")
    print(f"  非NULL: {nn:,} / {N:,} ({nn/N*100:.1f}%)")
    if nn == 0:
        continue
    v = s.dropna()
    qs = v.quantile([0, .01, .05, .25, .50, .75, .95, .99, 1.0])
    print("  分位: " + " ".join(f"p{int(q*100)}={qs[q]:.2f}" for q in qs.index))
    print(f"  mean={v.mean():.3f} std={v.std():.3f}")
    print(f"  ==0 の件数: {(v == 0).sum():,} ({(v==0).sum()/nn*100:.1f}%)")
    print(f"  |v|<1 の割合: {(v.abs() < 1).mean()*100:.1f}%   "
          f"|v|>100 の割合: {(v.abs() > 100).mean()*100:.2f}%   "
          f"v<=-100 の件数: {(v <= -100).sum():,}")
    # 整数値か小数値か (人数なら整数のはず)
    frac_int = np.isclose(v % 1, 0).mean()
    print(f"  整数値の割合: {frac_int*100:.1f}%  (人数単位なら100%のはず)")
    # 小数点以下の桁数
    dec = v.astype(str).str.split(".").str[-1].str.len()
    print(f"  小数桁数の最頻値: {dec.mode().tolist()[:3]} (最大 {dec.max()})")

# ------------------------------------------------- 3. 単位・分母の判定 (near-integer test)
hr("3. 単位と分母の判定: 増減人数が整数になるのはどの解釈か")
print("""前提: 従業員数は整数。よって「増減人数」が整数になる解釈が正しい。
  解釈A (対過去比 %):  d=(now-past)/past*100  → 増減 = now*d/(100+d)
  解釈B (対現在比 %):  d=(now-past)/now*100   → 増減 = now*d/100
  解釈C (対過去比 比率): d=(now-past)/past     → 増減 = now*d/(1+d)
  解釈D (人数そのもの):  増減 = d
delta は小数第2位まで丸められているため、丸め誤差が 0.1人 未満に収まる
規模帯 (employee_count 10〜1000) に限定して判定する。""")

sub = df[(df["employee_count"].between(10, 1000)) & df["employee_delta_1y"].notna()].copy()
sub["employee_delta_1y"] = pd.to_numeric(sub["employee_delta_1y"], errors="coerce")
sub = sub[sub["employee_delta_1y"].notna() & (sub["employee_delta_1y"] != 0)]
print(f"\n判定対象: {len(sub):,} 社 (employee_count 10-1000 かつ delta_1y が非0)")

now = sub["employee_count"].astype(float)
d = sub["employee_delta_1y"].astype(float)


def near_int_rate(x, tol=0.02):
    x = x.replace([np.inf, -np.inf], np.nan).dropna()
    return (np.abs(x - np.round(x)) < tol).mean(), len(x)


cands = {
    "A 対過去比%  now*d/(100+d)": now * d / (100.0 + d),
    "B 対現在比%  now*d/100": now * d / 100.0,
    "C 対過去比率 now*d/(1+d)": now * d / (1.0 + d),
    "D 人数そのもの d": d,
}
print(f"\n{'解釈':<28} {'増減が整数(±0.02)の割合':>22} {'対象':>8}")
for k, x in cands.items():
    r, n = near_int_rate(x)
    print(f"{k:<28} {r*100:>20.1f}% {n:>8,}")
print("\n(ランダムな連続値なら 4% 前後になるのが基準線)")

# さらに厳しい判定: 小規模企業ほど丸め誤差が小さいので規模別に見る
print("\n規模別の内訳 (解釈A vs 解釈B):")
print(f"{'employee_count':<20} {'社数':>8} {'A整数率':>10} {'B整数率':>10}")
for lo, hi in [(10, 50), (50, 100), (100, 300), (300, 1000)]:
    m = (now >= lo) & (now < hi)
    if m.sum() < 50:
        continue
    ra, _ = near_int_rate((now * d / (100.0 + d))[m])
    rb, _ = near_int_rate((now * d / 100.0)[m])
    print(f"{f'{lo}-{hi}':<20} {m.sum():>8,} {ra*100:>9.1f}% {rb*100:>9.1f}%")

# -------------------------------------------- 3b. 期間整合性 (2y と 1y の関係で追加検証)
hr("3b. 期間間の整合性チェック")
m = df[DELTAS].apply(pd.to_numeric, errors="coerce")
both = m[["employee_delta_1y", "employee_delta_2y"]].dropna()
print(f"1y と 2y が両方ある: {len(both):,} 社")
print(f"  |2y| >= |1y| の割合: {(both['employee_delta_2y'].abs() >= both['employee_delta_1y'].abs()).mean()*100:.1f}%")
print("  (%累積なら長期の方が振れ幅が大きい傾向が出るはず)")
print(f"\n1m と 1y の分散比較 (短期ほど小さいはず):")
for c in DELTAS:
    v = m[c].dropna()
    print(f"  {c:<22} std={v.std():>8.2f}  p95={v.quantile(.95):>8.2f}  p5={v.quantile(.05):>8.2f}")

# ---------------------------------------------------------------- 4. 欠損率と偏り
hr("4. employee_delta_* の欠損率と偏り")
d1y = pd.to_numeric(df["employee_delta_1y"], errors="coerce")
df["_has_d1y"] = d1y.notna()
print(f"delta_1y あり: {df['_has_d1y'].sum():,} / {N:,} ({df['_has_d1y'].mean()*100:.1f}%)")
ec = pd.to_numeric(df["employee_count"], errors="coerce")
print(f"employee_count あり: {ec.notna().sum():,} ({ec.notna().mean()*100:.1f}%)")
print(f"employee_count>0 かつ delta_1y あり: {((ec > 0) & df['_has_d1y']).sum():,} "
      f"({((ec > 0) & df['_has_d1y']).mean()*100:.1f}%)")

print("\n--- 規模帯別の delta_1y 保有率 ---")
g = df.groupby(df["employee_range"].fillna("(未設定)"), dropna=False)["_has_d1y"].agg(["size", "mean"])
g = g.sort_values("size", ascending=False)
for idx, row in g.head(12).iterrows():
    print(f"  {str(idx)[:40]:<42} n={row['size']:>7,.0f}  保有率={row['mean']*100:>5.1f}%")

print("\n--- 業種別の delta_1y 保有率 (上位15業種) ---")
g2 = df.groupby(df["sn_industry"].fillna("(未設定)"), dropna=False)["_has_d1y"].agg(["size", "mean"])
g2 = g2.sort_values("size", ascending=False)
for idx, row in g2.head(15).iterrows():
    print(f"  {str(idx)[:30]:<32} n={row['size']:>7,.0f}  保有率={row['mean']*100:>5.1f}%")
print(f"\n  保有率の業種間レンジ: min={g2[g2['size']>=200]['mean'].min()*100:.1f}% "
      f"max={g2[g2['size']>=200]['mean'].max()*100:.1f}% (n>=200 の業種)")

# ---------------------------------------------------------------- 5. collated_at
hr("5. collated_at の分布 (いつ時点のデータか)")
ca = pd.to_datetime(df["collated_at"], errors="coerce")
print(f"パース成功: {ca.notna().sum():,} / {N:,} ({ca.notna().mean()*100:.1f}%)")
if ca.notna().any():
    print(f"最小: {ca.min()}  最大: {ca.max()}  ユニーク日数: {ca.dt.date.nunique():,}")
    vc = ca.dt.date.value_counts().sort_index()
    print(f"\n  日付別件数 (上位10):")
    for dt_, c in ca.dt.date.value_counts().head(10).items():
        print(f"    {dt_}  {c:>8,} ({c/N*100:>5.1f}%)")
    print(f"\n  月別件数:")
    for mth, c in ca.dt.to_period("M").value_counts().sort_index().items():
        print(f"    {mth}  {c:>8,} ({c/N*100:>5.1f}%)")
    span = (ca.max() - ca.min()).days
    print(f"\n  収集期間の幅: {span} 日")

# ---------------------------------------------------------------- 6. 住所の質
hr("6. 住所から市区町村を切り出せるか")
mc = pd.read_csv(MASTER_CITY, dtype={"citycode": str, "prefcode": str})
PREF_BY_CODE = {}
print(f"master_city.csv: {len(mc):,} 行")

# 都道府県コード→名称 (address 先頭から推定)
pref_names = sorted(df["prefecture"].dropna().unique().tolist())
print(f"prefecture 列のユニーク値: {len(pref_names)} 種 -> {pref_names[:5]} ...")
print(f"prefecture 欠損: {df['prefecture'].isna().sum():,} ({df['prefecture'].isna().mean()*100:.1f}%)")
print(f"address 欠損: {df['address'].isna().sum():,} ({df['address'].isna().mean()*100:.1f}%)")

# prefcode -> prefecture名 を master_city と address から対応付け
code2pref = {}
for _, r in mc.iterrows():
    code2pref.setdefault(r["prefcode"], set()).add(r["city_name"])

# 都道府県名リスト (標準47)
PREFS = ["北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県", "茨城県", "栃木県",
         "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県", "新潟県", "富山県", "石川県", "福井県",
         "山梨県", "長野県", "岐阜県", "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府",
         "兵庫県", "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県", "徳島県",
         "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県",
         "鹿児島県", "沖縄県"]
pref2code = {p: str(i + 1) for i, p in enumerate(PREFS)}

# 都道府県ごとの市区町村名 (長い順に貪欲マッチ)
by_pref = {}
for _, r in mc.iterrows():
    by_pref.setdefault(str(int(r["prefcode"])), []).append(r["city_name"])
for k in by_pref:
    by_pref[k].sort(key=len, reverse=True)

addr = df["address"].fillna("")
pref = df["prefecture"].fillna("")


def extract_city(a, p):
    if not a:
        return None
    code = pref2code.get(p)
    if code is None:
        # address 先頭から都道府県を推定
        for pp in PREFS:
            if a.startswith(pp):
                code = pref2code[pp]
                p = pp
                break
    if code is None:
        return None
    rest = a[len(p):] if a.startswith(p) else a
    for city in by_pref.get(code, []):
        if rest.startswith(city):
            return (code, city)
    # 先頭一致しない場合、含有で再挑戦
    for city in by_pref.get(code, []):
        if city in rest[:20]:
            return (code, city)
    return None


print("\n市区町村を抽出中 (198k 行、少し時間がかかる)...", file=sys.stderr)
res = [extract_city(a, p) for a, p in zip(addr.tolist(), pref.tolist())]
df["_city"] = [r[1] if r else None for r in res]
df["_prefcode"] = [r[0] if r else None for r in res]
ok = df["_city"].notna()
print(f"\n市区町村の抽出成功: {ok.sum():,} / {N:,} ({ok.mean()*100:.1f}%)")
print(f"  address はあるが抽出失敗: {((addr != '') & ~ok).sum():,}")
print(f"  address 自体が空: {(addr == '').sum():,}")
print("\n  抽出失敗の address サンプル (10件):")
for a in df.loc[(addr != "") & ~ok, "address"].head(10):
    print(f"    {a[:60]}")

print(f"\ndelta_1y あり & 市区町村抽出成功: {(ok & df['_has_d1y']).sum():,} "
      f"({(ok & df['_has_d1y']).mean()*100:.1f}%)")

# ---------------------------------------------------------------- 7. 地域集計に耐えるか
hr("7. 市区町村ごとの企業数 (地域集計に耐えるか)")
cnt_all = df[ok].groupby(["_prefcode", "_city"]).size()
print(f"出現した市区町村: {len(cnt_all):,} / master_city {len(mc):,}")
q = cnt_all.quantile([0, .1, .25, .5, .75, .9, 1.0])
print("全企業ベースの市区町村あたり社数: " + " ".join(f"p{int(k*100)}={int(v)}" for k, v in q.items()))

usable = df[ok & df["_has_d1y"]]
cnt_u = usable.groupby(["_prefcode", "_city"]).size()
print(f"\ndelta_1y 保有企業のみ: 出現市区町村 {len(cnt_u):,}")
qu = cnt_u.quantile([0, .1, .25, .5, .75, .9, 1.0])
print("  市区町村あたり社数: " + " ".join(f"p{int(k*100)}={int(v)}" for k, v in qu.items()))
for th in [5, 10, 30, 50, 100]:
    print(f"  {th:>3}社未満の市区町村: {(cnt_u < th).sum():>5,} / {len(cnt_u):,} "
          f"({(cnt_u < th).mean()*100:>5.1f}%)")

print("\n  社数が多い市区町村 上位10:")
for (pc, c), n in cnt_u.sort_values(ascending=False).head(10).items():
    print(f"    {c:<16} {n:>7,}")
print("\n  社数が少ない市区町村 サンプル10:")
for (pc, c), n in cnt_u.sort_values().head(10).items():
    print(f"    {c:<16} {n:>7,}")

# 業種×市区町村まで割ると何社残るか
hr("7b. 業種 × 市区町村 まで割った場合")
cnt_ci = usable.groupby(["_prefcode", "_city", usable["sn_industry"].fillna("(未設定)")]).size()
print(f"組合せ数: {len(cnt_ci):,}")
qci = cnt_ci.quantile([.25, .5, .75, .9, 1.0])
print("  1組合せあたり社数: " + " ".join(f"p{int(k*100)}={int(v)}" for k, v in qci.items()))
print(f"  30社以上ある組合せ: {(cnt_ci >= 30).sum():,} ({(cnt_ci>=30).mean()*100:.1f}%)")

# ---------------------------------------------------------------- 8. 重心座標のカバー率
hr("8. municipality_centroids.csv とのマッチ率")
cen = pd.read_csv(CENTROIDS)
print(f"centroids: {len(cen):,} 行, name ユニーク {cen['name'].nunique():,}")
dup = cen["name"].value_counts()
print(f"  name が重複しているもの: {(dup > 1).sum()} 件 -> {dup[dup>1].head(10).index.tolist()}")
cen_names = set(cen["name"].tolist())
cities = cnt_u.reset_index()
cities.columns = ["prefcode", "city", "n"]
cities["matched"] = cities["city"].isin(cen_names)
print(f"\n企業が存在する市区町村のうち重心座標あり: {cities['matched'].sum():,} / {len(cities):,} "
      f"({cities['matched'].mean()*100:.1f}%)")
cov = cities.loc[cities["matched"], "n"].sum() / cities["n"].sum()
print(f"  企業件数ベースのカバー率: {cov*100:.1f}%")
print("\n  重心座標が無い市区町村 (企業数上位10):")
for _, r in cities[~cities["matched"]].sort_values("n", ascending=False).head(10).iterrows():
    print(f"    {r['city']:<16} {r['n']:>7,} 社")

# 同名市区町村が別都道府県に存在するか (重心が名前だけなので衝突リスク)
mc_dup = mc["city_name"].value_counts()
collide = set(mc_dup[mc_dup > 1].index)
cities["collide"] = cities["city"].isin(collide)
print(f"\n  同名が複数都道府県に存在する市区町村: {cities['collide'].sum():,} "
      f"({cities.loc[cities['collide'],'n'].sum():,} 社が該当)")
print(f"    例: {sorted(collide)[:10]}")

print("\n" + "=" * 78)
print("完了")
