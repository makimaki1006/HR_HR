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


# (テーブル名, 作成関数)。ここに足す
TABLES = [
    ("postings", create_postings),
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
