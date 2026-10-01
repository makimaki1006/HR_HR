#!/usr/bin/env python3
"""PR 用 E2E の fixture SQLite を生成する (実 DB = data/hellowork.db は使わない)。

使い方:  python scripts/e2e/make_fixture_db.py <出力先.db>   (既存ファイルは作り直す)

postings (63 行、決め打ち)。spec は下の値で具体的に assert できる。

  都道府県  市区町村  正社員  パート  計
  東京都    千代田区    10      5    15
  東京都    港区         8      4    12
  東京都    新宿区       6      4    10
  大阪府    大阪市      12      6    18
  大阪府    堺市         5      3     8
  合計 63 行 (東京都 37 / 大阪府 26、正社員 41 / パート 22)

  - employment_type は '正社員' / 'パート' の 2 値 (V2 の用語。'正職員' は使わない)。
  - 正社員の salary_min 合計: 東京都 5,680,000 (24 行) / 大阪府 4,160,000 (17 行)。
  - 各 (市区町村, 雇用形態) 内の k 番目 (0 始まり) の salary_min は
      正社員: 200000 + 10000*k (月給)   パート: 1000 + 50*k (時給)
    salary_max = salary_min + 50000 (正社員) / + 200 (パート)。
    例: 東京都 千代田区 正社員 (k=0..9) の salary_min は 200000..290000、平均 245000。
  - job_type は k が偶数なら '医療'、奇数なら '飲食業'。
  - 緯度経度・年間休日などは市区町村ごとに固定 (下の MUNICIPALITIES)。

採用診断 (recruitment_diag) 用の追加テーブル (postings は増やさない。63 行のまま):
  すべて「東京都 千代田区 (citycode 13101)」を中心にした行。ローカル DB だけ (Turso / SalesNow 無し) で
  採用診断の 9 API のうち 7 つが意味のある値を返す。Panel 4 (competitors) は SalesNow、
  Panel 6 (market_trend) は Turso が必要なので、Turso 無しでは必ず {"error": ...} を返す (fixture では作らない)。

  追加テーブルと行数:
    v2_flow_mesh1km_2021       (mesh1kmid, citycode, month, dayflag, timezone, population)    9 行
    v2_flow_fromto_city        (citycode, year, month, dayflag, timezone, from_area, population) 11 行
    v2_external_daytime_population (prefecture, municipality, daytime_pop)                   4 行
    v2_external_commute_od     (origin_pref, origin_muni, dest_pref, dest_muni, total_commuters) 15 行
    v2_external_labor_force    (prefecture, municipality, unemployed)                        13 行
    v2_vacancy_rate / v2_transparency_score                                                   各 1 行
  列は src/handlers/recruitment_diag/snapshot_tests.rs の create_rich_hw_db と同じ最小列。
  commute_od / daytime_population / labor_force は実 DB より列が少ない。これは意図的:
  Panel 8 (insights) の insight エンジンはこれらを「列が足りず SQL エラー → 空」として読むため、
  発火する示唆が HS-3 / HS-1 / AP-2 の 3 件に固定される (snapshot insights__rich_city.json と同じ結果)。
  Panel 3 / Panel 9 / Panel 7 は最小列で足りる列だけを読む。
  v2_flow_master_prefcity / v2_flow_city_agg / municipality_geocode は作らない (人流の示唆 SW-F* や
  通勤圏の示唆 CZ/CF が発火しないようにするため)。

  検索条件: 業種 飲食業 / 雇用形態 正社員 / 東京都 (prefcode 13) / 千代田区 (citycode 13101)
  (千代田区 正社員 10 行のうち、k が奇数 = 飲食業 は k=1,3,5,7,9 の 5 行。salary_min 210000..290000)
  期待値 (Rust の計算式を読んで手計算し、同じ SQL を python sqlite3 で流して一致を確認済み):
  Panel 1 difficulty      hw_count = 5、全国 正社員×飲食業 = 千代田 5 + 港 4 + 新宿 3 + 大阪市 6 + 堺 2 = 20
                           昼 = (22000+10000) x 2ヶ月 / 2 = 32000、夜 = 25000 x 2 / 2 = 25000
                           (dayflag=0 や timezone=2 の 999999 行、13103 の 777777 行は除外される)
                           score = 5 / 32000 x 10000 = 1.5625 (表示 1.6)、昼夜比 = 32000/25000 = 1.28 (<= 1.5 なので観光地補正なし)
                           rank 2 「穏やか」、全国比 5/20 = 0.25 (表示 25.00%)
  Panel 2 talent_pool     昼 32000 / 夜 25000 / 通勤流入 = 昼 - 夜 = 7000 / 昼夜比 1.28
  Panel 3 inflow          (画面は「開発中」。API は from_area 別: 0=6000 1=4000 2=3000 3=3000、合計 16000)
                           (year=2021, dayflag=1, timezone=0 の全月 SUM。noise の 777777 行は除外)
  Panel 5 condition_gap   業界 (千代田区 正社員 飲食業 5 行): 月給中央値 = salary_min 昇順の index 5//2=2 番目 = 250000、
                           賞与 2.0、年休 120 -> 年収中央値 250000 x (12+2.0) = 3,500,000、n=5
                           全業界 (千代田区 正社員 10 行): index 10//2=5 番目 = 250000 -> 3,500,000、n=10
                           自社 月給 28 万 / 年休 125 / 賞与 2.5 -> 年収 280000 x 14.5 = 4,060,000
                           差 = 4,060,000 - 3,500,000 = +560,000 (+16.0%)、年休 +5、賞与 +0.5
  Panel 7 opportunity_map 東京都 飲食業 正社員: 千代田区 5件/昼間人口 4000 x 10000 -> 12.5 標準、港区 4件/1600 x 10000 -> 25 激戦、
                           新宿区 3件/12000 x 10000 -> 2.5 穴場 (人口 1 万人あたり、区分は 5 / 20 で分ける)。スコア降順 = 港区, 千代田区, 新宿区 (3 件)
  Panel 8 insights        HS-3 (重大) / HS-1 (注意) / AP-2 (情報) の 3 件 (snapshot insights__rich_city.json と同値)
  Panel 9 talent_pool_expansion  千代田区 宛て OD の流入元 上位 12 のうち
                           30 分圏 (上位5) = 新宿区 文京区 台東区 渋谷区 中央区: 失業者 9000 / HW 10 / 5 市区町村
                           60 分圏 (次の7) = 江東区 港区 品川区 目黒区 世田谷区 豊島区 北区: 失業者 22400 / HW 12 / 7 市区町村
                           (自市区町村 99999、荒川区 5000 (13 番目)、別宛先の行は含まれない)
  注意: postings の employment_type は 'パート' 固定。採用診断は UI 値「パート」を
        'パート労働者' 等に展開するので、パート選択では Panel 1 / 5 / 7 が 0 件になる。検証は正社員で行う。

他チームが行を足すとき:
  1. 新しいテーブルは create_<table>(conn) 関数を作り、TABLES に (名前, 関数) を足す。
  2. postings に市区町村・行を足すなら MUNICIPALITIES に 1 行足す。
     ただし合計 63 などの既存の期待値 (smoke.spec.ts の /health db_rows など) が変わる。
     既存 spec を壊さないよう、足すときは期待値の定数 (tests/e2e/pr/helpers/fixture_values.ts) も更新する。
"""
import os
import sqlite3
import sys

# (都道府県, 市区町村, 正社員数, パート数, 緯度, 経度, 年間休日)
MUNICIPALITIES = [
    ("東京都", "千代田区", 10, 5, 35.6940, 139.7536, 120),
    ("東京都", "港区", 8, 4, 35.6581, 139.7514, 118),
    ("東京都", "新宿区", 6, 4, 35.6938, 139.7036, 115),
    ("大阪府", "大阪市", 12, 6, 34.6937, 135.5023, 112),
    ("大阪府", "堺市", 5, 3, 34.5733, 135.4830, 110),
]

POSTINGS_DDL = """
CREATE TABLE postings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_number TEXT,
    job_type TEXT NOT NULL,
    industry_raw TEXT,
    occupation_major TEXT,
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL,
    facility_name TEXT,
    employment_type TEXT,
    salary_type TEXT,
    salary_min INTEGER,
    salary_max INTEGER,
    annual_holidays INTEGER,
    bonus_months REAL,
    base_salary_min INTEGER,
    base_salary_max INTEGER,
    latitude REAL,
    longitude REAL,
    recruitment_reason TEXT,
    license_1 TEXT,
    license_2 TEXT,
    license_3 TEXT,
    headline TEXT,
    job_description TEXT,
    requirements TEXT,
    benefits TEXT,
    working_hours TEXT,
    holidays TEXT,
    access TEXT,
    hello_work_office TEXT
)
"""


def create_postings(conn):
    conn.execute(POSTINGS_DDL)
    n = 0
    for pref, muni, n_ft, n_pt, lat, lng, holidays in MUNICIPALITIES:
        for emp, count in (("正社員", n_ft), ("パート", n_pt)):
            for k in range(count):
                n += 1
                if emp == "正社員":
                    salary_type, smin, smax = "月給", 200000 + 10000 * k, 250000 + 10000 * k
                else:
                    salary_type, smin, smax = "時給", 1000 + 50 * k, 1200 + 50 * k
                job_type = "医療" if k % 2 == 0 else "飲食業"
                conn.execute(
                    "INSERT INTO postings (job_number, job_type, industry_raw, occupation_major,"
                    " prefecture, municipality, facility_name, employment_type, salary_type,"
                    " salary_min, salary_max, annual_holidays, bonus_months, base_salary_min,"
                    " base_salary_max, latitude, longitude, recruitment_reason)"
                    " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    (
                        f"E2E-{n:04d}", job_type, job_type, job_type,
                        pref, muni, f"{muni}事業所{k % 3 + 1}", emp, salary_type,
                        smin, smax, holidays, 2.0 if emp == "正社員" else 0.0, smin, smax,
                        lat, lng, "1" if k % 4 == 0 else "2",
                    ),
                )


# ---------------------------------------------------------------------------
# 採用診断 (recruitment_diag) 用テーブル。中心は 東京都 千代田区 (citycode 13101)
# ---------------------------------------------------------------------------

# 通勤 OD の流入元 (千代田区 宛て)。(都道府県, 市区町村, 通勤者数, 失業者数)。通勤者数の降順。
# 上位 5 = 30 分圏、次の 7 = 60 分圏。13 番目 (荒川区) は LIMIT 12 で落ちる。
COMMUTE_ORIGINS = [
    ("東京都", "新宿区", 90000, 2500),
    ("東京都", "文京区", 80000, 1800),
    ("東京都", "台東区", 70000, 1600),
    ("東京都", "渋谷区", 60000, 2200),
    ("東京都", "中央区", 50000, 900),
    ("東京都", "江東区", 40000, 4100),
    ("東京都", "港区", 35000, 3000),
    ("東京都", "品川区", 30000, 3500),
    ("東京都", "目黒区", 25000, 1700),
    ("東京都", "世田谷区", 20000, 5200),
    ("東京都", "豊島区", 15000, 2300),
    ("東京都", "北区", 10000, 2600),
    ("東京都", "荒川区", 5000, 1500),
]


def create_v2_flow_mesh1km_2021(conn):
    conn.execute(
        "CREATE TABLE v2_flow_mesh1km_2021 (mesh1kmid INTEGER, citycode INTEGER, month INTEGER,"
        " dayflag INTEGER, timezone INTEGER, population REAL)"
    )
    rows = []
    for month in (1, 2):
        rows += [
            (1, 13101, month, 1, 0, 22000),  # 平日昼 (mesh 1)
            (2, 13101, month, 1, 0, 10000),  # 平日昼 (mesh 2)  -> 月 32000
            (1, 13101, month, 1, 1, 25000),  # 平日深夜 -> 月 25000
        ]
    rows += [
        (1, 13101, 1, 0, 0, 999999),  # 休日昼: 集計に入ってはいけない
        (1, 13101, 1, 2, 2, 999999),  # 集計値 (dayflag=2): double count 防止で除外される
        (9, 13103, 1, 1, 0, 777777),  # 別の citycode (港区): 除外される
    ]
    conn.executemany("INSERT INTO v2_flow_mesh1km_2021 VALUES (?,?,?,?,?,?)", rows)


def create_v2_flow_fromto_city(conn):
    conn.execute(
        "CREATE TABLE v2_flow_fromto_city (citycode INTEGER, year INTEGER, month INTEGER,"
        " dayflag INTEGER, timezone INTEGER, from_area INTEGER, population REAL)"
    )
    rows = []
    for month in (1, 2):
        for from_area, pop in ((0, 3000), (1, 2000), (2, 1500), (3, 1500)):
            rows.append((13101, 2021, month, 1, 0, from_area, pop))
    rows += [
        (13101, 2021, 1, 1, 1, 0, 777777),  # 深夜: 除外
        (13101, 2020, 1, 1, 0, 0, 777777),  # 別の年: 除外
        (13101, 2021, 1, 0, 0, 0, 777777),  # 休日: 除外
    ]
    conn.executemany("INSERT INTO v2_flow_fromto_city VALUES (?,?,?,?,?,?,?)", rows)


def create_v2_external_daytime_population(conn):
    conn.execute(
        "CREATE TABLE v2_external_daytime_population (prefecture TEXT, municipality TEXT, daytime_pop REAL)"
    )
    conn.executemany(
        "INSERT INTO v2_external_daytime_population VALUES (?,?,?)",
        [
            ("東京都", "千代田区", 4000),
            ("東京都", "港区", 1600),
            ("東京都", "新宿区", 12000),
            ("大阪府", "大阪市", 50000),  # 別の都道府県: Panel 7 (東京都) に出ない
        ],
    )


def create_v2_external_commute_od(conn):
    conn.execute(
        "CREATE TABLE v2_external_commute_od (origin_pref TEXT, origin_muni TEXT, dest_pref TEXT,"
        " dest_muni TEXT, total_commuters INTEGER)"
    )
    rows = [("東京都", "千代田区", "東京都", "千代田区", 99999)]  # 自市区町村: 除外される
    rows += [(p, m, "東京都", "千代田区", c) for p, m, c, _ in COMMUTE_ORIGINS]
    rows += [("東京都", "新宿区", "東京都", "港区", 55555)]  # 別の宛先: 除外される
    conn.executemany("INSERT INTO v2_external_commute_od VALUES (?,?,?,?,?)", rows)


def create_v2_external_labor_force(conn):
    conn.execute("CREATE TABLE v2_external_labor_force (prefecture TEXT, municipality TEXT, unemployed INTEGER)")
    conn.executemany(
        "INSERT INTO v2_external_labor_force VALUES (?,?,?)",
        [(p, m, u) for p, m, _, u in COMMUTE_ORIGINS],
    )


def create_v2_vacancy_rate(conn):
    # Panel 8 の HS-1 (欠員補充率 25%) を発火させる。値は snapshot_tests.rs の rich DB と同じ
    conn.execute(
        "CREATE TABLE v2_vacancy_rate (prefecture TEXT, municipality TEXT, industry_raw TEXT, emp_group TEXT,"
        " total_count INTEGER, vacancy_count INTEGER, growth_count INTEGER, new_facility_count INTEGER,"
        " vacancy_rate REAL, growth_rate REAL)"
    )
    conn.execute(
        "INSERT INTO v2_vacancy_rate VALUES ('東京都', '千代田区', '', '正社員', 100, 25, 10, 2, 0.25, 0.10)"
    )


def create_v2_transparency_score(conn):
    # Panel 8 の HS-3 (開示度 30%、最低項目 残業時間 10%) と AP-2 を発火させる。値は rich DB と同じ
    conn.execute(
        "CREATE TABLE v2_transparency_score (prefecture TEXT, municipality TEXT, industry_raw TEXT, emp_group TEXT,"
        " total_count INTEGER, avg_transparency REAL, median_transparency REAL,"
        " disclosure_annual_holidays REAL, disclosure_bonus_months REAL, disclosure_employee_count REAL,"
        " disclosure_capital REAL, disclosure_overtime REAL, disclosure_female_ratio REAL,"
        " disclosure_parttime_ratio REAL, disclosure_founding_year REAL)"
    )
    conn.execute(
        "INSERT INTO v2_transparency_score VALUES ('東京都', '千代田区', '', '正社員', 100, 0.30, 0.25,"
        " 0.9, 0.6, 0.5, 0.4, 0.1, 0.2, 0.3, 0.35)"
    )


# (テーブル名, 作成関数)。ここに足す
TABLES = [
    ("postings", create_postings),
    ("v2_flow_mesh1km_2021", create_v2_flow_mesh1km_2021),
    ("v2_flow_fromto_city", create_v2_flow_fromto_city),
    ("v2_external_daytime_population", create_v2_external_daytime_population),
    ("v2_external_commute_od", create_v2_external_commute_od),
    ("v2_external_labor_force", create_v2_external_labor_force),
    ("v2_vacancy_rate", create_v2_vacancy_rate),
    ("v2_transparency_score", create_v2_transparency_score),
]


def build(path):
    if os.path.exists(path):
        os.remove(path)
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    conn = sqlite3.connect(path)
    try:
        for _, fn in TABLES:
            fn(conn)
        conn.commit()
        total = conn.execute("SELECT COUNT(*) FROM postings").fetchone()[0]
        expected = sum(m[2] + m[3] for m in MUNICIPALITIES)
        assert total == expected == 63, f"postings 行数が想定と違う: {total} / {expected}"
    finally:
        conn.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: make_fixture_db.py <out.db>")
    build(sys.argv[1])
    print(f"fixture written: {sys.argv[1]}")
