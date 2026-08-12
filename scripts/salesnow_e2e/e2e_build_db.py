# -*- coding: utf-8 -*-
"""E2E用: 実 CSV から v2_salesnow_companies テーブルを持つ SQLite を作る。
本番 Turso の代わりに libSQL 互換スタブ (e2e_turso_stub.py) が読む。
"""
import os
import sqlite3
import sys

import pandas as pd

CSV = r"C:\Users\fuji1\OneDrive\デスクトップ\HR_HR\data\salesnow_companies.csv"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "e2e_salesnow.db")

print("CSV 読込中 (全 46 列)...", file=sys.stderr)
# 列を絞ると本番に無い「列が存在しない」エラーで経路が落ちるため、CSV の全列を入れる
df = pd.read_csv(CSV, dtype={"corporate_number": str, "postal_code": str,
                             "hubspot_id": str, "jccode": str}, low_memory=False)
# 本番テーブルは company_name 列を持つ (fetch.rs が参照)
df["company_name"] = df["name"]

if os.path.exists(OUT):
    os.remove(OUT)
conn = sqlite3.connect(OUT)
df.to_sql("v2_salesnow_companies", conn, index=False)
conn.execute("CREATE INDEX idx_pref ON v2_salesnow_companies(prefecture)")
conn.execute("CREATE INDEX idx_pref_ind ON v2_salesnow_companies(prefecture, sn_industry)")
conn.commit()

n = conn.execute("SELECT COUNT(*) FROM v2_salesnow_companies").fetchone()[0]
print(f"作成: {OUT}")
print(f"  行数: {n:,}")
print(f"  列: {len(df.columns)}")
# 型を確認 (SQL の数値演算が効くか)
r = conn.execute("""SELECT typeof(employee_count), typeof(employee_delta_1y)
                    FROM v2_salesnow_companies
                    WHERE employee_count IS NOT NULL AND employee_delta_1y IS NOT NULL
                    LIMIT 1""").fetchone()
print(f"  typeof(employee_count)={r[0]}, typeof(employee_delta_1y)={r[1]}")
conn.close()
