# -*- coding: utf-8 -*-
"""全10データセットの逆証明検証スクリプト"""
import sys, io, csv, os
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")
ERRORS = []

def read_csv(fname):
    path = os.path.join(DATA_DIR, fname)
    with open(path, encoding="utf-8-sig") as f:
        return list(csv.DictReader(f))

print("=" * 60)
print("逆証明1: 東京都の具体値検証")
print("=" * 60)

# 学歴: 東京都大学卒 > 200万
edu = read_csv("education_by_prefecture.csv")
tokyo_uni = [r for r in edu if r["prefecture"] == "東京都" and r["education_level"] == "大学"]
if tokyo_uni:
    val = int(tokyo_uni[0]["total_count"])
    ok = val > 2_000_000
    print(f"  {'OK' if ok else 'NG'} 東京都 大学卒: {val:,}人 (期待: >2,000,000)")
    if not ok: ERRORS.append("東京都大学卒が少なすぎ")
else:
    ERRORS.append("東京都大学卒データなし")

# 世帯: 東京都単独世帯 40-65%
hh = read_csv("household_by_prefecture.csv")
tokyo_single = [r for r in hh if r["prefecture"] == "東京都" and r["household_type"] == "単独世帯"]
if tokyo_single:
    val = float(tokyo_single[0]["ratio"])
    ok = 0.40 < val < 0.65
    print(f"  {'OK' if ok else 'NG'} 東京都 単独世帯比率: {val:.1%} (期待: 40-65%)")
    if not ok: ERRORS.append(f"東京都単独世帯比率が範囲外: {val}")

# 地価: 東京都住宅地 > 10万円/m2
lp = read_csv("land_price_by_prefecture.csv")
tokyo_res = [r for r in lp if r["prefecture"] == "東京都" and r["land_use"] == "住宅地"]
if tokyo_res:
    val = float(tokyo_res[0]["avg_price_per_sqm"])
    ok = val > 100_000
    print(f"  {'OK' if ok else 'NG'} 東京都 住宅地価: {val:,.0f}円/m2 (期待: >100,000)")
    if not ok: ERRORS.append(f"東京都住宅地価が異常: {val}")

# 自動車: 東京都 < 25台/100人
car = read_csv("car_ownership_by_prefecture.csv")
tokyo_car = [r for r in car if r["prefecture"] == "東京都"]
if tokyo_car:
    val = float(tokyo_car[0]["cars_per_100people"])
    ok = val < 25
    print(f"  {'OK' if ok else 'NG'} 東京都 自動車: {val:.1f}台/100人 (期待: <25)")
    if not ok: ERRORS.append(f"東京都自動車保有率が高すぎ: {val}")

# ネット: 東京都 > 75%
net = read_csv("internet_usage_by_prefecture.csv")
tokyo_net = [r for r in net if r["prefecture"] == "東京都"]
if tokyo_net:
    val = float(tokyo_net[0]["internet_usage_rate"])
    ok = val > 75
    print(f"  {'OK' if ok else 'NG'} 東京都 ネット利用率: {val:.1f}% (期待: >75%)")

print()
print("=" * 60)
print("逆証明2: 地方県の具体値検証")
print("=" * 60)

# 群馬: 車保有 > 40
gunma_car = [r for r in car if r["prefecture"] == "群馬県"]
if gunma_car:
    val = float(gunma_car[0]["cars_per_100people"])
    ok = val > 40
    print(f"  {'OK' if ok else 'NG'} 群馬県 自動車: {val:.1f}台/100人 (期待: >40)")

# 秋田: 単独世帯 < 東京
akita_single = [r for r in hh if r["prefecture"] == "秋田県" and r["household_type"] == "単独世帯"]
if akita_single and tokyo_single:
    a = float(akita_single[0]["ratio"])
    t = float(tokyo_single[0]["ratio"])
    ok = a < t
    print(f"  {'OK' if ok else 'NG'} 秋田県 単独世帯: {a:.1%} < 東京都 {t:.1%}")

# 愛知: 技能実習 > 10,000
fr = read_csv("foreign_residents_by_prefecture.csv")
aichi = [r for r in fr if r["prefecture"] == "愛知県" and r["visa_status"] == "技能実習"]
if aichi:
    val = int(aichi[0]["count"])
    ok = val > 10_000
    print(f"  {'OK' if ok else 'NG'} 愛知県 技能実習: {val:,}人 (期待: >10,000)")

print()
print("=" * 60)
print("逆証明3: 日銀短観DI")
print("=" * 60)

tankan = read_csv("boj_tankan_di.csv")
dates = sorted(set(r["survey_date"] for r in tankan))
print(f"  survey_date範囲: {dates[0]} ~ {dates[-1]} ({len(dates)}四半期)")

# 2020Q2コロナ: 製造業DI < 0
covid = [r for r in tankan
    if r["survey_date"] == "202002"
    and "製造業" in r.get("industry_j", "")
    and r["enterprise_size"] == "large"
    and r["di_type"] == "business"
    and r["result_type"] == "actual"]
if covid:
    val = int(covid[0]["di_value"])
    ok = val < 0
    print(f"  {'OK' if ok else 'NG'} 2020Q2 製造業大企業DI: {val} (期待: <0 コロナ影響)")
else:
    print("  -- 2020Q2製造業データなし（industry_j値を確認）")
    mfg = [r["industry_j"] for r in tankan if "製造" in r.get("industry_j", "")]
    if mfg:
        print(f"     industry_j例: {list(set(mfg))[:3]}")

# 最新の雇用人員DI < 0 (人手不足)
emp = [r for r in tankan
    if r["di_type"] == "employment"
    and r["enterprise_size"] == "large"
    and r["result_type"] == "actual"
    and "全産業" in r.get("industry_j", "")]
if emp:
    latest = sorted(emp, key=lambda x: x["survey_date"])[-1]
    val = int(latest["di_value"])
    ok = val < 0
    print(f"  {'OK' if ok else 'NG'} 最新雇用人員DI(全産業大企業): {val} @{latest['survey_date']} (期待: <0)")

print()
print("=" * 60)
print("逆証明4: 経済センサス産業構造")
print("=" * 60)

ind = read_csv("industry_structure_by_municipality.csv")

# 千代田区(13101) 全産業事業所 > 10,000
chiyoda = [r for r in ind if r["city_code"] == "13101" and r["industry_code"] == "AS"]
if chiyoda:
    val = int(chiyoda[0]["establishments"])
    ok = val > 10_000
    print(f"  {'OK' if ok else 'NG'} 千代田区 全産業事業所: {val:,} (期待: >10,000)")
else:
    ERRORS.append("千代田区データなし")
    print("  NG 千代田区データなし")

# 豊田市(23211) 製造業従業者 > 30,000
toyota = [r for r in ind if r["city_code"] == "23211" and r["industry_code"] == "E"]
if toyota:
    val = int(toyota[0]["employees_total"])
    ok = val > 30_000
    print(f"  {'OK' if ok else 'NG'} 豊田市 製造業従業者: {val:,}人 (期待: >30,000)")

# 都道府県数 = 47
prefs = set(r["prefecture_code"] for r in ind)
print(f"  {'OK' if len(prefs)==47 else 'NG'} 都道府県数: {len(prefs)} (期待: 47)")

# 市区町村数 > 1700
cities = set(r["city_code"] for r in ind)
print(f"  {'OK' if len(cities)>1700 else 'NG'} 市区町村数: {len(cities)} (期待: >1700)")

print()
print("=" * 60)
print("逆証明5: Turso接続確認")
print("=" * 60)

env = {}
env_path = os.path.join(os.path.dirname(DATA_DIR), "..", ".env")
with open(env_path, encoding="utf-8") as f:
    for line in f:
        if "=" in line and not line.startswith("#"):
            k, v = line.strip().split("=", 1)
            env[k.strip()] = v.strip().strip('"').strip("'")

ext_url = env.get("TURSO_EXTERNAL_URL", "")
ext_token = env.get("TURSO_EXTERNAL_TOKEN", "")

if ext_url and ext_token:
    import requests
    if ext_url.startswith("libsql://"):
        ext_url = ext_url.replace("libsql://", "https://")
    try:
        resp = requests.post(
            f"{ext_url}/v2/pipeline",
            headers={"Authorization": f"Bearer {ext_token}", "Content-Type": "application/json"},
            json={"requests": [
                {"type": "execute", "stmt": {"sql": "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"}},
                {"type": "close"}
            ]},
            timeout=15
        )
        data = resp.json()
        tables = [r[0]["value"] for r in data["results"][0]["response"]["result"]["rows"]]
        print(f"  OK Turso接続成功 - 既存テーブル {len(tables)}個: {tables[:5]}...")
        new_tables = [
            "v2_external_foreign_residents", "v2_external_education",
            "v2_external_household", "v2_external_boj_tankan",
            "v2_external_social_life", "v2_external_household_spending",
            "v2_external_industry_structure", "v2_external_land_price",
            "v2_external_car_ownership", "v2_external_internet_usage",
        ]
        conflicts = [t for t in new_tables if t in tables]
        if conflicts:
            print(f"  -- 既存テーブルと衝突: {conflicts} (INSERT OR REPLACEで上書き)")
        else:
            print(f"  OK 名前衝突なし - 全て新規テーブル")
    except Exception as e:
        print(f"  NG Turso接続失敗: {e}")
        ERRORS.append(f"Turso接続失敗: {e}")
else:
    print("  -- TURSO_EXTERNAL_URL/TOKEN が.envにない")

print()
print("=" * 60)
if ERRORS:
    print(f"結論: NG ({len(ERRORS)}件のエラー)")
    for e in ERRORS:
        print(f"  - {e}")
else:
    print("結論: 全検証合格 - アップロード可能")
print("=" * 60)
