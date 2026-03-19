"""
e-Stat APIから都道府県別の事業所動態（存続・新設・廃業）データを取得し、
hellowork.dbのv2_external_business_dynamicsテーブルに格納する。

データソース: 経済センサス（活動調査・基礎調査）
- 2009年: 経済センサス基礎調査 (statsDataId=0003032609)
- 2012年: 経済センサス活動調査 (statsDataId=0003094560)
- 2014年: 経済センサス基礎調査 (statsDataId=0003111171)
- 2016年: 経済センサス活動調査 (statsDataId=0003218711)
- 2021年: 経済センサス活動調査 (statsDataId=0004005669)
"""

import json
import sqlite3
import sys
import time
import urllib.request
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
BASE_URL = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
DB_PATH = Path(r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db")

# 都道府県コード → 都道府県名のマッピング
PREF_MAP = {
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


def fetch_estat_data(stats_data_id: str, params: dict, timeout: int = 120) -> dict:
    """e-Stat APIからデータを取得する"""
    query_params = {
        "appId": APP_ID,
        "lang": "J",
        "statsDataId": stats_data_id,
        **params,
    }
    query_str = "&".join(f"{k}={v}" for k, v in query_params.items())
    url = f"{BASE_URL}?{query_str}"
    print(f"  リクエスト: {url[:120]}...")
    with urllib.request.urlopen(url, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def safe_int(val: str) -> int | None:
    """安全に整数変換する。"-"や空文字はNoneを返す"""
    if val is None or val == "-" or val == "" or val == "…" or val == "x":
        return None
    try:
        return int(val)
    except (ValueError, TypeError):
        return None


def extract_pref_data(value_list: list, area_key: str = "area") -> dict:
    """APIレスポンスのVALUEから都道府県別データを抽出する

    Returns:
        {area_code: {tab_code: value, ...}, ...}
    """
    result = {}
    for item in value_list:
        area_code = item.get(f"@{area_key}")
        if area_code not in PREF_MAP:
            continue
        tab_code = item.get("@tab", item.get("@cat01", ""))
        val = item.get("$", "")
        if area_code not in result:
            result[area_code] = {}
        result[area_code][tab_code] = val
    return result


def fetch_2009() -> list[dict]:
    """2009年 経済センサス基礎調査 (statsDataId=0003032609)
    tab: 027=総数, 028=存続, 029=新設, 030=廃業
    area: 都道府県コード(01000等)
    """
    print("\n[2009年] 経済センサス基礎調査を取得中...")
    # 都道府県コードをカンマ区切りで指定
    pref_codes = ",".join(PREF_MAP.keys())
    data = fetch_estat_data("0003032609", {
        "cdTab": "027,028,029,030",
        "cdArea": pref_codes,
    })

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data["DATA_INF"]["VALUE"]
    if not isinstance(values, list):
        values = [values]

    pref_data = extract_pref_data(values, "area")

    results = []
    for area_code, tabs in pref_data.items():
        pref = PREF_MAP[area_code]
        total = safe_int(tabs.get("027"))
        survived = safe_int(tabs.get("028"))
        new_est = safe_int(tabs.get("029"))
        closed = safe_int(tabs.get("030"))

        # 開業率・廃業率の計算（前回調査時点の事業所数 = 存続 + 廃業）
        base_count = None
        if survived is not None and closed is not None:
            base_count = survived + closed

        opening_rate = None
        if new_est is not None and base_count and base_count > 0:
            opening_rate = round(new_est / base_count * 100, 2)

        closure_rate = None
        if closed is not None and base_count and base_count > 0:
            closure_rate = round(closed / base_count * 100, 2)

        net_change = None
        if new_est is not None and closed is not None:
            net_change = new_est - closed

        results.append({
            "prefecture": pref,
            "fiscal_year": "2009",
            "total_establishments": total,
            "new_establishments": new_est,
            "closed_establishments": closed,
            "survived_establishments": survived,
            "net_change": net_change,
            "opening_rate": opening_rate,
            "closure_rate": closure_rate,
        })

    print(f"  取得件数: {len(results)} 都道府県")
    return results


def fetch_2012() -> list[dict]:
    """2012年 経済センサス活動調査 (statsDataId=0003094560)
    tab: 985=総数(不詳含), 986=存続, 987=新設, 988=廃業, 989=総数
    cat01: 地域コード (area ではない)
    """
    print("\n[2012年] 経済センサス活動調査を取得中...")
    pref_codes = ",".join(PREF_MAP.keys())
    data = fetch_estat_data("0003094560", {
        "cdTab": "985,986,987,988",
        "cdCat01": pref_codes,
    })

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data["DATA_INF"]["VALUE"]
    if not isinstance(values, list):
        values = [values]

    # このデータセットでは地域がcat01に入っている
    result_map = {}
    for item in values:
        area_code = item.get("@cat01", "")
        if area_code not in PREF_MAP:
            continue
        tab_code = item.get("@tab", "")
        val = item.get("$", "")
        if area_code not in result_map:
            result_map[area_code] = {}
        result_map[area_code][tab_code] = val

    results = []
    for area_code, tabs in result_map.items():
        pref = PREF_MAP[area_code]
        total = safe_int(tabs.get("985"))
        survived = safe_int(tabs.get("986"))
        new_est = safe_int(tabs.get("987"))
        closed = safe_int(tabs.get("988"))

        base_count = None
        if survived is not None and closed is not None:
            base_count = survived + closed

        opening_rate = None
        if new_est is not None and base_count and base_count > 0:
            opening_rate = round(new_est / base_count * 100, 2)

        closure_rate = None
        if closed is not None and base_count and base_count > 0:
            closure_rate = round(closed / base_count * 100, 2)

        net_change = None
        if new_est is not None and closed is not None:
            net_change = new_est - closed

        results.append({
            "prefecture": pref,
            "fiscal_year": "2012",
            "total_establishments": total,
            "new_establishments": new_est,
            "closed_establishments": closed,
            "survived_establishments": survived,
            "net_change": net_change,
            "opening_rate": opening_rate,
            "closure_rate": closure_rate,
        })

    print(f"  取得件数: {len(results)} 都道府県")
    return results


def fetch_2014() -> list[dict]:
    """2014年 経済センサス基礎調査 (statsDataId=0003111171)
    tab: 027=総数, 028=存続, 029=新設, 030=廃業
    cat01: 単独・本所・支所 (000=総数)
    cat02: 経営組織 (001=民営)
    cat03: 産業分類 (010=全産業)
    area: 都道府県コード
    """
    print("\n[2014年] 経済センサス基礎調査を取得中...")
    pref_codes = ",".join(PREF_MAP.keys())
    data = fetch_estat_data("0003111171", {
        "cdTab": "027,028,029,030",
        "cdCat01": "000",
        "cdCat02": "001",
        "cdCat03": "010",
        "cdArea": pref_codes,
    })

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data["DATA_INF"]["VALUE"]
    if not isinstance(values, list):
        values = [values]

    pref_data = extract_pref_data(values, "area")

    results = []
    for area_code, tabs in pref_data.items():
        pref = PREF_MAP[area_code]
        total = safe_int(tabs.get("027"))
        survived = safe_int(tabs.get("028"))
        new_est = safe_int(tabs.get("029"))
        closed = safe_int(tabs.get("030"))

        base_count = None
        if survived is not None and closed is not None:
            base_count = survived + closed

        opening_rate = None
        if new_est is not None and base_count and base_count > 0:
            opening_rate = round(new_est / base_count * 100, 2)

        closure_rate = None
        if closed is not None and base_count and base_count > 0:
            closure_rate = round(closed / base_count * 100, 2)

        net_change = None
        if new_est is not None and closed is not None:
            net_change = new_est - closed

        results.append({
            "prefecture": pref,
            "fiscal_year": "2014",
            "total_establishments": total,
            "new_establishments": new_est,
            "closed_establishments": closed,
            "survived_establishments": survived,
            "net_change": net_change,
            "opening_rate": opening_rate,
            "closure_rate": closure_rate,
        })

    print(f"  取得件数: {len(results)} 都道府県")
    return results


def fetch_2016() -> list[dict]:
    """2016年 経済センサス活動調査 (statsDataId=0003218711)
    tab: 4002=総数(不詳含), 4004=存続, 4006=新設, 4008=廃業
    area: 都道府県コード
    """
    print("\n[2016年] 経済センサス活動調査を取得中...")
    pref_codes = ",".join(PREF_MAP.keys())
    data = fetch_estat_data("0003218711", {
        "cdTab": "4002,4004,4006,4008",
        "cdArea": pref_codes,
    })

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data["DATA_INF"]["VALUE"]
    if not isinstance(values, list):
        values = [values]

    pref_data = extract_pref_data(values, "area")

    results = []
    for area_code, tabs in pref_data.items():
        pref = PREF_MAP[area_code]
        total = safe_int(tabs.get("4002"))
        survived = safe_int(tabs.get("4004"))
        new_est = safe_int(tabs.get("4006"))
        closed = safe_int(tabs.get("4008"))

        base_count = None
        if survived is not None and closed is not None:
            base_count = survived + closed

        opening_rate = None
        if new_est is not None and base_count and base_count > 0:
            opening_rate = round(new_est / base_count * 100, 2)

        closure_rate = None
        if closed is not None and base_count and base_count > 0:
            closure_rate = round(closed / base_count * 100, 2)

        net_change = None
        if new_est is not None and closed is not None:
            net_change = new_est - closed

        results.append({
            "prefecture": pref,
            "fiscal_year": "2016",
            "total_establishments": total,
            "new_establishments": new_est,
            "closed_establishments": closed,
            "survived_establishments": survived,
            "net_change": net_change,
            "opening_rate": opening_rate,
            "closure_rate": closure_rate,
        })

    print(f"  取得件数: {len(results)} 都道府県")
    return results


def fetch_2021() -> list[dict]:
    """2021年 経済センサス活動調査 (statsDataId=0004005669)
    tab: 101-2021=総数(不詳含), 102-2021=事業所数
    cat01: 0=総数, 1=存続, 2=新設, 3=廃業
    area: 都道府県コード
    """
    print("\n[2021年] 経済センサス活動調査を取得中...")
    pref_codes = ",".join(PREF_MAP.keys())
    data = fetch_estat_data("0004005669", {
        "cdTab": "101-2021",
        "cdCat01": "0,1,2,3",
        "cdArea": pref_codes,
    })

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data["DATA_INF"]["VALUE"]
    if not isinstance(values, list):
        values = [values]

    # {area_code: {cat01_code: value}}
    result_map = {}
    for item in values:
        area_code = item.get("@area", "")
        if area_code not in PREF_MAP:
            continue
        cat01 = item.get("@cat01", "")
        val = item.get("$", "")
        if area_code not in result_map:
            result_map[area_code] = {}
        result_map[area_code][cat01] = val

    results = []
    for area_code, cats in result_map.items():
        pref = PREF_MAP[area_code]
        total = safe_int(cats.get("0"))
        survived = safe_int(cats.get("1"))
        new_est = safe_int(cats.get("2"))
        closed = safe_int(cats.get("3"))

        base_count = None
        if survived is not None and closed is not None:
            base_count = survived + closed

        opening_rate = None
        if new_est is not None and base_count and base_count > 0:
            opening_rate = round(new_est / base_count * 100, 2)

        closure_rate = None
        if closed is not None and base_count and base_count > 0:
            closure_rate = round(closed / base_count * 100, 2)

        net_change = None
        if new_est is not None and closed is not None:
            net_change = new_est - closed

        results.append({
            "prefecture": pref,
            "fiscal_year": "2021",
            "total_establishments": total,
            "new_establishments": new_est,
            "closed_establishments": closed,
            "survived_establishments": survived,
            "net_change": net_change,
            "opening_rate": opening_rate,
            "closure_rate": closure_rate,
        })

    print(f"  取得件数: {len(results)} 都道府県")
    return results


def create_table(conn: sqlite3.Connection) -> None:
    """テーブルを作成する"""
    conn.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_business_dynamics (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            total_establishments INTEGER,
            new_establishments INTEGER,
            closed_establishments INTEGER,
            survived_establishments INTEGER,
            net_change INTEGER,
            opening_rate REAL,
            closure_rate REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """)
    conn.commit()
    print("テーブル v2_external_business_dynamics を作成（または既存確認）しました。")


def insert_data(conn: sqlite3.Connection, all_data: list[dict]) -> int:
    """データを挿入する（既存データは置換）"""
    inserted = 0
    for row in all_data:
        conn.execute("""
            INSERT OR REPLACE INTO v2_external_business_dynamics
            (prefecture, fiscal_year, total_establishments, new_establishments,
             closed_establishments, survived_establishments, net_change,
             opening_rate, closure_rate)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            row["prefecture"],
            row["fiscal_year"],
            row["total_establishments"],
            row["new_establishments"],
            row["closed_establishments"],
            row["survived_establishments"],
            row["net_change"],
            row["opening_rate"],
            row["closure_rate"],
        ))
        inserted += 1
    conn.commit()
    return inserted


def verify_data(conn: sqlite3.Connection) -> None:
    """格納データを検証する"""
    print("\n=== データ検証 ===")

    # 年度別件数
    rows = conn.execute("""
        SELECT fiscal_year, COUNT(*) as cnt
        FROM v2_external_business_dynamics
        GROUP BY fiscal_year
        ORDER BY fiscal_year
    """).fetchall()
    print("\n年度別レコード数:")
    for year, cnt in rows:
        print(f"  {year}年: {cnt} 都道府県")

    # 全体統計
    total = conn.execute("SELECT COUNT(*) FROM v2_external_business_dynamics").fetchone()[0]
    print(f"\n総レコード数: {total}")

    # 2021年のサンプル（廃業率上位5都道府県）
    print("\n2021年 廃業率上位5都道府県:")
    rows = conn.execute("""
        SELECT prefecture, total_establishments, new_establishments,
               closed_establishments, net_change, opening_rate, closure_rate
        FROM v2_external_business_dynamics
        WHERE fiscal_year = '2021'
        ORDER BY closure_rate DESC
        LIMIT 5
    """).fetchall()
    for pref, total, new, closed, net, o_rate, c_rate in rows:
        print(f"  {pref}: 総数={total:,}, 新設={new:,}, 廃業={closed:,}, "
              f"純増減={net:+,}, 開業率={o_rate}%, 廃業率={c_rate}%")

    # 2021年のサンプル（廃業数上位5都道府県）
    print("\n2021年 廃業数上位5都道府県:")
    rows = conn.execute("""
        SELECT prefecture, total_establishments, new_establishments,
               closed_establishments, net_change, opening_rate, closure_rate
        FROM v2_external_business_dynamics
        WHERE fiscal_year = '2021'
        ORDER BY closed_establishments DESC
        LIMIT 5
    """).fetchall()
    for pref, total, new, closed, net, o_rate, c_rate in rows:
        print(f"  {pref}: 総数={total:,}, 新設={new:,}, 廃業={closed:,}, "
              f"純増減={net:+,}, 開業率={o_rate}%, 廃業率={c_rate}%")

    # 東京都の時系列
    print("\n東京都の時系列推移:")
    rows = conn.execute("""
        SELECT fiscal_year, total_establishments, new_establishments,
               closed_establishments, net_change, opening_rate, closure_rate
        FROM v2_external_business_dynamics
        WHERE prefecture = '東京都'
        ORDER BY fiscal_year
    """).fetchall()
    for year, total, new, closed, net, o_rate, c_rate in rows:
        total_s = f"{total:,}" if total else "N/A"
        new_s = f"{new:,}" if new else "N/A"
        closed_s = f"{closed:,}" if closed else "N/A"
        net_s = f"{net:+,}" if net is not None else "N/A"
        print(f"  {year}年: 総数={total_s}, 新設={new_s}, 廃業={closed_s}, "
              f"純増減={net_s}, 開業率={o_rate}%, 廃業率={c_rate}%")


def main():
    print("=" * 60)
    print("e-Stat API: 都道府県別事業所動態データ取得")
    print("=" * 60)

    # 全年度のデータを取得
    all_data = []

    fetchers = [
        ("2009", fetch_2009),
        ("2012", fetch_2012),
        ("2014", fetch_2014),
        ("2016", fetch_2016),
        ("2021", fetch_2021),
    ]

    for year, fetcher in fetchers:
        try:
            data = fetcher()
            all_data.extend(data)
            time.sleep(1)  # API負荷軽減
        except Exception as e:
            print(f"  [警告] {year}年のデータ取得に失敗: {e}")
            import traceback
            traceback.print_exc()

    if not all_data:
        print("\nデータが取得できませんでした。処理を中断します。")
        sys.exit(1)

    print(f"\n合計 {len(all_data)} レコードを取得しました。")

    # DB格納
    print(f"\nDB格納先: {DB_PATH}")
    conn = sqlite3.connect(str(DB_PATH))
    try:
        create_table(conn)
        inserted = insert_data(conn, all_data)
        print(f"{inserted} レコードを格納しました。")
        verify_data(conn)
    finally:
        conn.close()

    print("\n処理完了。")


if __name__ == "__main__":
    main()
