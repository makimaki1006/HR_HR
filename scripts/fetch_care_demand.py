"""
e-Stat API から介護需要指標データを取得し hellowork.db に格納する。

社会・人口統計体系（社人統）から以下のデータを都道府県別・年度別に取得:
- 介護保険給付（件数・費用額・支給額）
- 介護施設（特養・老健・訪問介護事業所数等）
- 高齢者人口（65歳以上・75歳以上）
- 後期高齢者医療
"""

import json
import sqlite3
import sys
import time
import urllib.request
from pathlib import Path

APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
DB_PATH = Path(r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db")

# 都道府県コードと名称のマッピング
PREF_CODE_TO_NAME = {
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

# 取得する指標定義
# statsDataId => { cat01_code: (db_column, description) }
INDICATORS = {
    # J分野: 福祉・社会保障 (statsDataId=0000010110)
    "0000010110": {
        "J7101":   ("insurance_benefit_cases",         "介護保険給付（件数）"),
        "J710101": ("insurance_benefit_cases_home",     "居宅介護サービス給付（件数）"),
        "J710102": ("insurance_benefit_cases_facility", "施設介護サービス給付（件数）"),
        "J7102":   ("insurance_benefit_cost",          "介護保険給付（費用額・千円）"),
        "J710201": ("insurance_benefit_cost_home",      "居宅介護サービス給付（費用額・千円）"),
        "J710202": ("insurance_benefit_cost_facility",  "施設介護サービス給付（費用額・千円）"),
        "J230121": ("nursing_home_count",              "介護老人福祉施設数（特養）"),
        "J230124": ("nursing_home_capacity",           "介護老人福祉施設定員数"),
        "J230125": ("nursing_home_residents",          "介護老人福祉施設在所者数"),
        "J230155": ("home_care_offices",               "訪問介護事業所数"),
        "J230156": ("home_care_users",                 "訪問介護利用者数"),
        "J230165": ("day_service_offices",             "通所介護事業所数"),
        "J230168": ("day_service_users",               "通所介護利用者数"),
        "J230181": ("care_support_offices",            "居宅介護支援事業所数"),
        "J230182": ("care_support_users",              "居宅介護支援事業利用者数"),
        "J4501":   ("late_elderly_insured",            "後期高齢者医療被保険者数"),
        "J4503":   ("late_elderly_medical_cost",       "後期高齢者医療費（千円）"),
        "J3108":   ("home_helper_count",               "訪問介護員（ホームヘルパー）数"),
    },
    # I分野: 保健衛生 (statsDataId=0000010109)
    "0000010109": {
        "I5501":   ("health_facility_count",           "介護老人保健施設数"),
        "I5502":   ("health_facility_capacity",        "介護老人保健施設定員数"),
        "I5503":   ("health_facility_residents",       "介護老人保健施設在所者数"),
    },
    # A分野: 人口 (statsDataId=0000010101)
    "0000010101": {
        "A1303":   ("pop_65_over",                     "65歳以上人口"),
        "A1306":   ("pop_65_over_rate",                "65歳以上人口割合（%）"),
        "A1419":   ("pop_75_over",                     "75歳以上人口"),
    },
}


def fetch_estat_data(stats_data_id: str, cat01_codes: list[str]) -> list[dict]:
    """e-Stat APIからデータを取得する。"""
    all_values = []
    codes_str = ",".join(cat01_codes)

    # 全都道府県 + 全国を取得（areaは指定しない）
    # 年度は2013年度以降に限定（最新10年分程度）
    base_url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
        f"?appId={APP_ID}&lang=J&statsDataId={stats_data_id}"
        f"&cdCat01={codes_str}"
        f"&limit=100000"
    )

    print(f"  API呼び出し: statsDataId={stats_data_id}, cat01={len(cat01_codes)}件")

    try:
        with urllib.request.urlopen(base_url, timeout=120) as resp:
            data = json.loads(resp.read().decode("utf-8"))

        stat_data = data.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
        result_info = stat_data.get("RESULT_INF", {})
        total = result_info.get("TOTAL_NUMBER", 0)
        print(f"    取得件数: {total}")

        values = stat_data.get("DATA_INF", {}).get("VALUE", [])
        if not isinstance(values, list):
            values = [values]
        all_values.extend(values)

        # ページング処理
        next_key = result_info.get("NEXT_KEY")
        while next_key:
            print(f"    ページング: NEXT_KEY={next_key}")
            page_url = f"{base_url}&startPosition={next_key}"
            time.sleep(1)  # API負荷軽減
            with urllib.request.urlopen(page_url, timeout=120) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            stat_data = data.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
            result_info = stat_data.get("RESULT_INF", {})
            values = stat_data.get("DATA_INF", {}).get("VALUE", [])
            if not isinstance(values, list):
                values = [values]
            all_values.extend(values)
            next_key = result_info.get("NEXT_KEY")

    except Exception as e:
        print(f"    エラー: {e}")

    return all_values


def parse_time_code(time_code: str) -> str:
    """時間コードを年度文字列に変換する。例: 2020100000 -> 2020"""
    return time_code[:4]


def parse_value(val_str: str) -> float | None:
    """値文字列を数値に変換する。"""
    if not val_str or val_str in ("-", "…", "***", "x", "X", "*"):
        return None
    try:
        # カンマ除去
        cleaned = val_str.replace(",", "")
        return float(cleaned)
    except (ValueError, TypeError):
        return None


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    print("=" * 60)
    print("介護需要指標データ取得開始")
    print("=" * 60)

    # 全指標のデータを収集
    # {(prefecture, fiscal_year): {column: value, ...}}
    collected: dict[tuple[str, str], dict] = {}

    for stats_data_id, indicators in INDICATORS.items():
        cat01_codes = list(indicators.keys())
        print(f"\n--- statsDataId={stats_data_id} ({len(cat01_codes)}指標) ---")

        values = fetch_estat_data(stats_data_id, cat01_codes)
        time.sleep(2)  # API負荷軽減

        for v in values:
            area_code = v.get("@area", "")
            time_code = v.get("@time", "")
            cat01_code = v.get("@cat01", "")
            val_str = v.get("$", "")

            # 全国(00000)と47都道府県のみ
            if area_code not in PREF_CODE_TO_NAME and area_code != "00000":
                continue

            # 2013年度以降のみ
            fiscal_year = parse_time_code(time_code)
            try:
                if int(fiscal_year) < 2013:
                    continue
            except ValueError:
                continue

            prefecture = PREF_CODE_TO_NAME.get(area_code, "全国")
            val = parse_value(val_str)

            if cat01_code in indicators:
                col_name = indicators[cat01_code][0]
                key = (prefecture, fiscal_year)
                if key not in collected:
                    collected[key] = {}
                collected[key][col_name] = val

    print(f"\n合計レコード数: {len(collected)}")

    # サンプルデータ表示（東京都 2020年度）
    sample_key = ("東京都", "2020")
    if sample_key in collected:
        print(f"\nサンプル（東京都 2020年度）:")
        for col, val in sorted(collected[sample_key].items()):
            print(f"  {col}: {val}")

    # DB格納
    print(f"\nDB格納: {DB_PATH}")
    if not DB_PATH.parent.exists():
        print(f"エラー: ディレクトリが存在しません: {DB_PATH.parent}")
        sys.exit(1)

    # 全カラム名を収集（ソート済みリスト）
    all_columns_set = set()
    for stats_data_id, indicators in INDICATORS.items():
        for cat01_code, (col_name, _desc) in indicators.items():
            all_columns_set.add(col_name)
    sorted_cols = sorted(all_columns_set)

    # テーブル作成
    col_defs = [f"    {col} REAL" for col in sorted_cols]

    create_sql = (
        "CREATE TABLE IF NOT EXISTS v2_external_care_demand (\n"
        "    prefecture TEXT NOT NULL,\n"
        "    fiscal_year TEXT NOT NULL,\n"
        + ",\n".join(col_defs)
        + ",\n    PRIMARY KEY (prefecture, fiscal_year)\n)"
    )

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    # 既存テーブルをドロップして再作成
    cur.execute("DROP TABLE IF EXISTS v2_external_care_demand")
    cur.execute(create_sql)
    print(f"  テーブル作成完了: v2_external_care_demand ({len(sorted_cols)}カラム)")

    # CREATE SQLの検証
    cur.execute("PRAGMA table_info(v2_external_care_demand)")
    db_cols = [row[1] for row in cur.fetchall()]
    print(f"  DBカラム: {db_cols}")

    # データ挿入
    placeholders = ", ".join(["?"] * (2 + len(sorted_cols)))
    col_names = ", ".join(["prefecture", "fiscal_year"] + sorted_cols)
    insert_sql = f"INSERT OR REPLACE INTO v2_external_care_demand ({col_names}) VALUES ({placeholders})"

    rows = []
    for (pref, fy), data in collected.items():
        row = [pref, fy]
        for col in sorted_cols:
            row.append(data.get(col))
        rows.append(row)

    cur.executemany(insert_sql, rows)
    conn.commit()
    print(f"  挿入行数: {len(rows)}")

    # 検証
    cur.execute("SELECT COUNT(*) FROM v2_external_care_demand")
    total = cur.fetchone()[0]
    cur.execute("SELECT COUNT(DISTINCT prefecture) FROM v2_external_care_demand")
    pref_count = cur.fetchone()[0]
    cur.execute("SELECT COUNT(DISTINCT fiscal_year) FROM v2_external_care_demand")
    year_count = cur.fetchone()[0]
    cur.execute("SELECT MIN(fiscal_year), MAX(fiscal_year) FROM v2_external_care_demand")
    min_year, max_year = cur.fetchone()

    print(f"\n--- 検証結果 ---")
    print(f"  総行数: {total}")
    print(f"  都道府県数: {pref_count}")
    print(f"  年度数: {year_count}")
    print(f"  年度範囲: {min_year} ~ {max_year}")

    # 東京都のサンプル表示
    cur.execute(
        "SELECT * FROM v2_external_care_demand WHERE prefecture = '東京都' ORDER BY fiscal_year DESC LIMIT 3"
    )
    cols_desc = [desc[0] for desc in cur.description]
    print(f"\nサンプル（東京都 最新3年度）:")
    for row in cur.fetchall():
        print(f"  {row[1]}年度:")
        for i, col in enumerate(cols_desc):
            if i >= 2 and row[i] is not None:
                print(f"    {col}: {row[i]:,.0f}" if isinstance(row[i], (int, float)) and row[i] > 1 else f"    {col}: {row[i]}")

    # 全47都道府県の存在確認
    cur.execute("SELECT DISTINCT prefecture FROM v2_external_care_demand WHERE prefecture != '全国' ORDER BY prefecture")
    prefs = [r[0] for r in cur.fetchall()]
    print(f"\n都道府県一覧 ({len(prefs)}件):")
    print(f"  {', '.join(prefs[:10])} ...")

    conn.close()
    print("\n完了")


if __name__ == "__main__":
    main()
