"""
雇用動向調査（statsCode: 00450073）から都道府県別の入職率・離職率データを取得し、
hellowork.db に格納するスクリプト。

データソース:
- 都道府県別入職者数 (statsDataId=0003376330)
- 都道府県別離職者数 (statsDataId=0003376376)
- 都道府県別常用労働者数 (statsDataId=0003376318)
- 産業別入職・離職率 (statsDataId=0003401233) ※全国レベル
- 地域別入職・離職率 (statsDataId=0003377224) ※地域ブロック単位

入職率 = 入職者数 / 常用労働者数 × 100
離職率 = 離職者数 / 常用労働者数 × 100
"""

import urllib.request
import json
import sqlite3
import sys
import time

sys.stdout.reconfigure(encoding="utf-8")

APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
BASE_URL = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
DB_PATH = r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db"

# 都道府県コード→名称マッピング
PREF_CODE_MAP = {
    "00000": "全国",
    "01000": "北海道", "02000": "青森県", "03000": "岩手県", "04000": "宮城県",
    "05000": "秋田県", "06000": "山形県", "07000": "福島県", "08000": "茨城県",
    "09000": "栃木県", "10000": "群馬県", "11000": "埼玉県", "12000": "千葉県",
    "13000": "東京都", "14000": "神奈川県", "15000": "新潟県", "16000": "富山県",
    "17000": "石川県", "18000": "福井県", "19000": "山梨県", "20000": "長野県",
    "21000": "岐阜県", "22000": "静岡県", "23000": "愛知県", "24000": "三重県",
    "25000": "滋賀県", "26000": "京都府", "27000": "大阪府", "28000": "兵庫県",
    "29000": "奈良県", "30000": "和歌山県", "31000": "鳥取県", "32000": "島根県",
    "33000": "岡山県", "34000": "広島県", "35000": "山口県", "36000": "徳島県",
    "37000": "香川県", "38000": "愛媛県", "39000": "高知県", "40000": "福岡県",
    "41000": "佐賀県", "42000": "長崎県", "43000": "熊本県", "44000": "大分県",
    "45000": "宮崎県", "46000": "鹿児島県", "47000": "沖縄県",
}

# 産業コード→名称マッピング
INDUSTRY_CODE_MAP = {
    "S100": "産業計",
    "000C": "鉱業，採石業，砂利採取業",
    "000D": "建設業",
    "000E": "製造業",
    "000F": "電気・ガス・熱供給・水道業",
    "000G": "情報通信業",
    "000H": "運輸業，郵便業",
    "000I": "卸売業，小売業",
    "000J": "金融業，保険業",
    "000K": "不動産業，物品賃貸業",
    "000L": "学術研究，専門・技術サービス業",
    "000M": "宿泊業，飲食サービス業",
    "000N": "生活関連サービス業，娯楽業",
    "000O": "教育，学習支援業",
    "000P": "医療，福祉",
    "000Q": "複合サービス事業",
    "000R": "サービス業（他に分類されないもの）",
}

# 地域コード→名称マッピング
REGION_CODE_MAP = {
    "80100": "全国",
    "01000": "北海道",
    "80110": "東北",
    "80120": "北関東",
    "80130": "南関東",
    "80140": "北陸",
    "80150": "東海",
    "80160": "近畿",
    "80170": "京阪神",
    "80180": "山陰",
    "80190": "山陽",
    "80200": "四国",
    "80210": "北九州",
    "80220": "南九州",
}


def fetch_estat_data(stats_data_id: str, params: dict = None) -> list:
    """e-Stat APIからデータを取得（ページネーション対応）"""
    all_values = []
    start_pos = 1
    limit = 10000

    while True:
        url_params = (
            f"appId={APP_ID}&lang=J&statsDataId={stats_data_id}"
            f"&limit={limit}&startPosition={start_pos}"
        )
        if params:
            for k, v in params.items():
                url_params += f"&{k}={v}"

        url = f"{BASE_URL}?{url_params}"
        req = urllib.request.Request(url)

        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                data = json.loads(resp.read().decode("utf-8"))
        except Exception as e:
            print(f"  API呼び出しエラー: {e}")
            break

        stat_data = data.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
        result_inf = stat_data.get("RESULT_INF", {})
        total = int(result_inf.get("TOTAL_NUMBER", 0))
        values = stat_data.get("DATA_INF", {}).get("VALUE", [])

        if not values:
            break

        all_values.extend(values)
        next_key = result_inf.get("NEXT_KEY")
        if next_key:
            start_pos = int(next_key)
            time.sleep(1)  # API負荷軽減
        else:
            break

    return all_values


def fetch_prefectural_turnover():
    """都道府県別の入職者数・離職者数・常用労働者数を取得し、率を計算"""
    print("=" * 60)
    print("1. 都道府県別データの取得")
    print("=" * 60)

    # 入職者数は2021年まで、離職者数は2019年まで → 共通年次は2014-2019
    years = "2019000000,2018000000,2017000000,2016000000,2015000000,2014000000"
    common_params = {
        "cdCat02": "100",  # 性別: 合計
        "cdCat03": "100",  # 調査周期: 年計
        "cdCat01": "S100,000P",  # 産業計 + 医療福祉
        "cdTime": years,
    }

    # 入職者数
    print("\n[1-1] 都道府県別入職者数を取得中...")
    entry_data = fetch_estat_data("0003376330", common_params)
    print(f"  取得件数: {len(entry_data)}")

    # 離職者数
    print("\n[1-2] 都道府県別離職者数を取得中...")
    time.sleep(1)
    sep_data = fetch_estat_data("0003376376", common_params)
    print(f"  取得件数: {len(sep_data)}")

    # 常用労働者数 - 同じパラメータ構造（cat01=産業, cat02=性別, cat03=調査周期）
    print("\n[1-3] 都道府県別常用労働者数を取得中...")
    time.sleep(1)
    worker_params = {
        "cdCat01": "S100,000P",  # 産業計 + 医療福祉
        "cdCat02": "100",  # 性別: 合計
        "cdCat03": "100",  # 調査周期: 年計
        "cdTime": years,
    }
    worker_data = fetch_estat_data("0003376318", worker_params)
    print(f"  取得件数: {len(worker_data)}")

    # データを辞書に変換
    # 入職者数: key = (area, cat01, time) → value（千人）
    entry_dict = {}
    for v in entry_data:
        area = v.get("@area", "")
        cat01 = v.get("@cat01", "")
        year = v.get("@time", "")[:4]
        val = v.get("$", "")
        if val and val != "-" and val != "…" and val != "x":
            try:
                entry_dict[(area, cat01, year)] = float(val)
            except ValueError:
                pass

    # 離職者数
    sep_dict = {}
    for v in sep_data:
        area = v.get("@area", "")
        cat01 = v.get("@cat01", "")
        year = v.get("@time", "")[:4]
        val = v.get("$", "")
        if val and val != "-" and val != "…" and val != "x":
            try:
                sep_dict[(area, cat01, year)] = float(val)
            except ValueError:
                pass

    # 常用労働者数（cat01=産業、cat02=性別）
    worker_dict = {}
    for v in worker_data:
        area = v.get("@area", "")
        cat01 = v.get("@cat01", "")  # 産業
        year = v.get("@time", "")[:4]
        val = v.get("$", "")
        if val and val != "-" and val != "…" and val != "x":
            try:
                worker_dict[(area, cat01, year)] = float(val)
            except ValueError:
                pass

    # 都道府県別の入職率・離職率を計算
    # 入職率 = 入職者数 / 常用労働者数 × 100
    results = []
    for area_code, pref_name in PREF_CODE_MAP.items():
        for industry_code, industry_name in [("S100", "産業計"), ("000P", "医療，福祉")]:
            for year in ["2019", "2018", "2017", "2016", "2015", "2014"]:
                entry = entry_dict.get((area_code, industry_code, year))
                sep = sep_dict.get((area_code, industry_code, year))
                worker = worker_dict.get((area_code, industry_code, year))

                if worker and worker > 0:
                    entry_rate = round((entry / worker) * 100, 1) if entry else None
                    sep_rate = round((sep / worker) * 100, 1) if sep else None
                    net_rate = None
                    if entry_rate is not None and sep_rate is not None:
                        net_rate = round(entry_rate - sep_rate, 1)

                    results.append({
                        "prefecture": pref_name,
                        "fiscal_year": year,
                        "industry": industry_name,
                        "entry_rate": entry_rate,
                        "separation_rate": sep_rate,
                        "net_rate": net_rate,
                        "entry_count": entry,  # 千人
                        "separation_count": sep,  # 千人
                        "worker_count": worker,  # 千人
                    })

    print(f"\n  計算結果: {len(results)} 件")

    # サンプル表示
    print("\n  --- サンプル（2021年・産業計） ---")
    for r in results:
        if r["fiscal_year"] == "2019" and r["industry"] == "産業計" and r["prefecture"] in ["全国", "東京都", "大阪府", "北海道"]:
            print(f"  {r['prefecture']}: 入職率={r['entry_rate']}% 離職率={r['separation_rate']}% 純増減={r['net_rate']}%")

    print("\n  --- サンプル（2021年・医療福祉） ---")
    for r in results:
        if r["fiscal_year"] == "2019" and r["industry"] == "医療，福祉" and r["prefecture"] in ["全国", "東京都", "大阪府", "北海道"]:
            print(f"  {r['prefecture']}: 入職率={r['entry_rate']}% 離職率={r['separation_rate']}% 純増減={r['net_rate']}%")

    return results


def fetch_industry_turnover_rate():
    """産業別入職・離職率（全国レベル、直接率データ）を取得"""
    print("\n" + "=" * 60)
    print("2. 産業別入職・離職率（全国レベル）の取得")
    print("=" * 60)

    params = {"cdTime": "2017000000,2016000000,2015000000,2014000000,2013000000"}
    data = fetch_estat_data("0003401233", params)
    print(f"  取得件数: {len(data)}")

    results = []
    for v in data:
        tab = v.get("@tab", "")
        cat01 = v.get("@cat01", "")
        year = v.get("@time", "")[:4]
        val_str = v.get("$", "")
        industry = INDUSTRY_CODE_MAP.get(cat01, cat01)

        if val_str and val_str not in ("-", "…", "x"):
            try:
                val = float(val_str)
            except ValueError:
                continue

            rate_type = "entry_rate" if tab == "200" else "separation_rate"
            key = (industry, year)

            # 既存エントリを探す
            found = None
            for r in results:
                if r["industry"] == industry and r["fiscal_year"] == year:
                    found = r
                    break
            if found is None:
                found = {
                    "industry": industry,
                    "fiscal_year": year,
                    "entry_rate": None,
                    "separation_rate": None,
                    "net_rate": None,
                }
                results.append(found)
            found[rate_type] = val

    # net_rate計算
    for r in results:
        if r["entry_rate"] is not None and r["separation_rate"] is not None:
            r["net_rate"] = round(r["entry_rate"] - r["separation_rate"], 1)

    print(f"  整形結果: {len(results)} 件")

    # サンプル表示
    print("\n  --- サンプル（2017年） ---")
    for r in sorted(results, key=lambda x: (x["fiscal_year"], x["industry"])):
        if r["fiscal_year"] == "2017":
            print(f"  {r['industry']}: 入職率={r['entry_rate']}% 離職率={r['separation_rate']}% 純増減={r['net_rate']}%")

    return results


def fetch_region_turnover_rate():
    """地域別入職・離職率（直接率データ）を取得"""
    print("\n" + "=" * 60)
    print("3. 地域別入職・離職率の取得")
    print("=" * 60)

    params = {"cdTime": "2017000000,2016000000,2015000000,2014000000,2013000000"}
    data = fetch_estat_data("0003377224", params)
    print(f"  取得件数: {len(data)}")

    results = []
    for v in data:
        tab = v.get("@tab", "")
        area = v.get("@area", "")
        year = v.get("@time", "")[:4]
        val_str = v.get("$", "")
        region = REGION_CODE_MAP.get(area, area)

        if val_str and val_str not in ("-", "…", "x"):
            try:
                val = float(val_str)
            except ValueError:
                continue

            rate_type = "entry_rate" if tab == "200" else "separation_rate"

            found = None
            for r in results:
                if r["region"] == region and r["fiscal_year"] == year:
                    found = r
                    break
            if found is None:
                found = {
                    "region": region,
                    "fiscal_year": year,
                    "entry_rate": None,
                    "separation_rate": None,
                    "net_rate": None,
                }
                results.append(found)
            found[rate_type] = val

    for r in results:
        if r["entry_rate"] is not None and r["separation_rate"] is not None:
            r["net_rate"] = round(r["entry_rate"] - r["separation_rate"], 1)

    print(f"  整形結果: {len(results)} 件")

    print("\n  --- サンプル（2017年） ---")
    for r in sorted(results, key=lambda x: x["region"]):
        if r["fiscal_year"] == "2017":
            print(f"  {r['region']}: 入職率={r['entry_rate']}% 離職率={r['separation_rate']}% 純増減={r['net_rate']}%")

    return results


def store_to_db(pref_data, industry_data, region_data):
    """データをhellowork.dbに格納"""
    print("\n" + "=" * 60)
    print("4. DB格納")
    print("=" * 60)

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    # テーブル1: 都道府県別入職・離職（産業計 + 医療福祉）
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_turnover (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            industry TEXT NOT NULL DEFAULT '産業計',
            entry_rate REAL,
            separation_rate REAL,
            net_rate REAL,
            entry_count REAL,
            separation_count REAL,
            worker_count REAL,
            PRIMARY KEY (prefecture, fiscal_year, industry)
        )
    """)

    # テーブル2: 産業別入職・離職率（全国）
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_turnover_by_industry (
            industry TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            entry_rate REAL,
            separation_rate REAL,
            net_rate REAL,
            PRIMARY KEY (industry, fiscal_year)
        )
    """)

    # テーブル3: 地域別入職・離職率
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_turnover_by_region (
            region TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            entry_rate REAL,
            separation_rate REAL,
            net_rate REAL,
            PRIMARY KEY (region, fiscal_year)
        )
    """)

    # 既存データ削除
    cur.execute("DELETE FROM v2_external_turnover")
    cur.execute("DELETE FROM v2_external_turnover_by_industry")
    cur.execute("DELETE FROM v2_external_turnover_by_region")

    # 都道府県別データ挿入
    inserted_pref = 0
    for r in pref_data:
        if r["entry_rate"] is not None or r["separation_rate"] is not None:
            cur.execute(
                """INSERT OR REPLACE INTO v2_external_turnover
                   (prefecture, fiscal_year, industry, entry_rate, separation_rate,
                    net_rate, entry_count, separation_count, worker_count)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    r["prefecture"], r["fiscal_year"], r["industry"],
                    r["entry_rate"], r["separation_rate"], r["net_rate"],
                    r["entry_count"], r["separation_count"], r["worker_count"],
                ),
            )
            inserted_pref += 1

    # 産業別データ挿入
    inserted_ind = 0
    for r in industry_data:
        if r["entry_rate"] is not None or r["separation_rate"] is not None:
            cur.execute(
                """INSERT OR REPLACE INTO v2_external_turnover_by_industry
                   (industry, fiscal_year, entry_rate, separation_rate, net_rate)
                   VALUES (?, ?, ?, ?, ?)""",
                (r["industry"], r["fiscal_year"], r["entry_rate"],
                 r["separation_rate"], r["net_rate"]),
            )
            inserted_ind += 1

    # 地域別データ挿入
    inserted_reg = 0
    for r in region_data:
        if r["entry_rate"] is not None or r["separation_rate"] is not None:
            cur.execute(
                """INSERT OR REPLACE INTO v2_external_turnover_by_region
                   (region, fiscal_year, entry_rate, separation_rate, net_rate)
                   VALUES (?, ?, ?, ?, ?)""",
                (r["region"], r["fiscal_year"], r["entry_rate"],
                 r["separation_rate"], r["net_rate"]),
            )
            inserted_reg += 1

    conn.commit()

    print(f"\n  v2_external_turnover: {inserted_pref} 件挿入")
    print(f"  v2_external_turnover_by_industry: {inserted_ind} 件挿入")
    print(f"  v2_external_turnover_by_region: {inserted_reg} 件挿入")

    # 検証
    print("\n--- 検証 ---")

    print("\n[v2_external_turnover] 都道府県別:")
    cur.execute("SELECT COUNT(*) FROM v2_external_turnover")
    print(f"  総行数: {cur.fetchone()[0]}")
    cur.execute("SELECT COUNT(DISTINCT prefecture) FROM v2_external_turnover")
    print(f"  都道府県数: {cur.fetchone()[0]}")
    cur.execute("SELECT DISTINCT fiscal_year FROM v2_external_turnover ORDER BY fiscal_year")
    years = [r[0] for r in cur.fetchall()]
    print(f"  年次: {', '.join(years)}")
    cur.execute("SELECT DISTINCT industry FROM v2_external_turnover ORDER BY industry")
    industries = [r[0] for r in cur.fetchall()]
    print(f"  産業: {', '.join(industries)}")

    print("\n  サンプル（2021年・産業計・主要都道府県）:")
    cur.execute("""
        SELECT prefecture, entry_rate, separation_rate, net_rate
        FROM v2_external_turnover
        WHERE fiscal_year = '2019' AND industry = '産業計'
          AND prefecture IN ('全国', '東京都', '大阪府', '北海道', '福岡県')
        ORDER BY prefecture
    """)
    for row in cur.fetchall():
        print(f"    {row[0]}: 入職率={row[1]}% 離職率={row[2]}% 純増減={row[3]}%")

    print("\n  サンプル（2021年・医療福祉・主要都道府県）:")
    cur.execute("""
        SELECT prefecture, entry_rate, separation_rate, net_rate
        FROM v2_external_turnover
        WHERE fiscal_year = '2019' AND industry = '医療，福祉'
          AND prefecture IN ('全国', '東京都', '大阪府', '北海道', '福岡県')
        ORDER BY prefecture
    """)
    for row in cur.fetchall():
        print(f"    {row[0]}: 入職率={row[1]}% 離職率={row[2]}% 純増減={row[3]}%")

    print("\n[v2_external_turnover_by_industry] 産業別:")
    cur.execute("SELECT COUNT(*) FROM v2_external_turnover_by_industry")
    print(f"  総行数: {cur.fetchone()[0]}")
    print("\n  サンプル（2017年・主要産業）:")
    cur.execute("""
        SELECT industry, entry_rate, separation_rate, net_rate
        FROM v2_external_turnover_by_industry
        WHERE fiscal_year = '2017'
        ORDER BY separation_rate DESC
    """)
    for row in cur.fetchall():
        print(f"    {row[0]}: 入職率={row[1]}% 離職率={row[2]}% 純増減={row[3]}%")

    print("\n[v2_external_turnover_by_region] 地域別:")
    cur.execute("SELECT COUNT(*) FROM v2_external_turnover_by_region")
    print(f"  総行数: {cur.fetchone()[0]}")
    print("\n  サンプル（2017年）:")
    cur.execute("""
        SELECT region, entry_rate, separation_rate, net_rate
        FROM v2_external_turnover_by_region
        WHERE fiscal_year = '2017'
        ORDER BY separation_rate DESC
    """)
    for row in cur.fetchall():
        print(f"    {row[0]}: 入職率={row[1]}% 離職率={row[2]}% 純増減={row[3]}%")

    conn.close()
    print("\n完了")


def main():
    print("雇用動向調査データ取得・DB格納スクリプト")
    print("出典: e-Stat 雇用動向調査（statsCode: 00450073）")
    print()

    # 1. 都道府県別データ
    pref_data = fetch_prefectural_turnover()

    # 2. 産業別データ
    industry_data = fetch_industry_turnover_rate()

    # 3. 地域別データ
    region_data = fetch_region_turnover_rate()

    # 4. DB格納
    store_to_db(pref_data, industry_data, region_data)


if __name__ == "__main__":
    main()
