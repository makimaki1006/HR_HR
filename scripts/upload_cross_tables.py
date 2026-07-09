# -*- coding: utf-8 -*-
"""
クロス集計 3 テーブルを Turso (country-statistics) に投入する。

対象:
  scripts/staging/cross_future_workforce.csv → cross_future_workforce (約1,884行)
  scripts/staging/cross_wage_public.csv      → cross_wage_public      (約576行)
  scripts/staging/cross_switcher_supply.csv  → cross_switcher_supply  (約134行)

使い方 (PowerShell):
  $env:TURSO_EXTERNAL_TOKEN = "<トークン>"
  python scripts/upload_cross_tables.py          # 初回投入 (既にデータがあれば中断)
  python scripts/upload_cross_tables.py --force  # 作り直し (DROP して再投入)

安全設計:
- 既にテーブルに行がある場合は誤上書き防止のため中断する (--force 指定時のみ置き換え)
- 投入後に行数と実データのサンプル・スポット検証を自動実行する
- 「Turso アップロードは 1 回で完了」ルール: 投入前にローカル CSV の検証を行い、
  問題があれば投入せずに終了する

出典 (レポート掲出時の表示義務):
- 国の将来人口推計 (国立社会保障・人口問題研究所 令和5年推計)
- 総務省 就業構造基本調査 (2022年)
- 厚労省 毎月勤労統計 地方調査 / 地域別最低賃金
- 厚労省 一般職業紹介状況 (有効求人倍率)
"""
import csv
import io
import os
import sys

sys.stdout.reconfigure(encoding="utf-8")

URL = os.environ.get(
    "TURSO_EXTERNAL_URL",
    "libsql://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io",
)
TOKEN = os.environ.get("TURSO_EXTERNAL_TOKEN", "").strip()
FORCE = "--force" in sys.argv

BASE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "staging")

# テーブル定義 (CSV 列と 1:1。型は実データに合わせて明示)
TABLES = {
    "cross_future_workforce": {
        "csv": "cross_future_workforce.csv",
        "ddl": """CREATE TABLE cross_future_workforce (
            muni_code TEXT PRIMARY KEY,
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            wa_2020 INTEGER,
            wa_2040 INTEGER,
            wa_decline_rate REAL,
            working_age_ratio_2020 REAL,
            aged75_2020 INTEGER,
            aged75_2040 INTEGER,
            aged75_growth REAL,
            pop_index_2040 REAL,
            quadrant TEXT
        )""",
        "int_cols": {"wa_2020", "wa_2040", "aged75_2020", "aged75_2040"},
        "real_cols": {"wa_decline_rate", "working_age_ratio_2020", "aged75_growth", "pop_index_2040"},
        "expect_rows": (1800, 1950),
    },
    "cross_wage_public": {
        "csv": "cross_wage_public.csv",
        "ddl": """CREATE TABLE cross_wage_public (
            prefecture TEXT NOT NULL,
            year_month TEXT NOT NULL,
            scheduled_earnings INTEGER,
            min_wage_hourly INTEGER,
            min_wage_monthly_160h INTEGER,
            PRIMARY KEY (prefecture, year_month)
        )""",
        "int_cols": {"scheduled_earnings", "min_wage_hourly", "min_wage_monthly_160h"},
        "real_cols": set(),
        "expect_rows": (500, 700),
    },
    "cross_switcher_supply": {
        "csv": "cross_switcher_supply.csv",
        "ddl": """CREATE TABLE cross_switcher_supply (
            region_code TEXT PRIMARY KEY,
            region_name TEXT NOT NULL,
            employed_total INTEGER,
            job_change_seekers INTEGER,
            job_change_desire_rate REAL,
            additional_job_seekers INTEGER,
            side_job_holders INTEGER,
            pref_job_openings_ratio REAL
        )""",
        "int_cols": {"employed_total", "job_change_seekers", "additional_job_seekers", "side_job_holders"},
        "real_cols": {"job_change_desire_rate", "pref_job_openings_ratio"},
        "expect_rows": (120, 150),
    },
}

# 投入前ローカル検証 (スポット値はモック検証時と一致すること)
SPOT_CHECKS = [
    ("cross_future_workforce", lambda rows: any(
        r["municipality"] == "大分市" and abs(float(r["wa_decline_rate"]) + 14.1) < 0.2 for r in rows
    ), "大分市の働き手減少率が -14.1% 前後"),
    ("cross_wage_public", lambda rows: any(
        r["prefecture"] == "大分県" and r["year_month"] == "2025-12"
        and int(r["scheduled_earnings"]) == 239448 for r in rows
    ), "大分県 2025-12 の平均給与 239,448円"),
    ("cross_wage_public", lambda rows: all(
        r["prefecture"].endswith(("都", "道", "府", "県")) or r["prefecture"] == "全国" for r in rows
    ), "都道府県名がフルネーム (「大分」ではなく「大分県」)"),
    ("cross_switcher_supply", lambda rows: any(
        r["region_name"] == "大分県" and abs(float(r["job_change_desire_rate"]) - 8.84) < 0.05 for r in rows
    ), "大分県の転職を考えている割合 8.84%"),
]


def load_csv(path):
    with io.open(path, encoding="utf-8-sig") as f:
        return list(csv.DictReader(f))


def sql_quote(v):
    if v is None or v == "":
        return "NULL"
    return "'" + str(v).replace("'", "''") + "'"


def main():
    if not TOKEN:
        print("[中断] 環境変数 TURSO_EXTERNAL_TOKEN が設定されていません。")
        print('  PowerShell:  $env:TURSO_EXTERNAL_TOKEN = "<トークン>"')
        return 1

    # ---- 1. ローカル検証 (投入前に全チェック。失敗したら投入しない) ----
    print("=== 1. 投入前のローカル検証 ===")
    data = {}
    for table, spec in TABLES.items():
        path = os.path.join(BASE, spec["csv"])
        if not os.path.exists(path):
            print(f"[中断] CSV がありません: {path}")
            return 1
        rows = load_csv(path)
        lo, hi = spec["expect_rows"]
        if not (lo <= len(rows) <= hi):
            print(f"[中断] {table}: 行数 {len(rows)} が想定範囲 {lo}-{hi} 外")
            return 1
        data[table] = rows
        print(f"  {table}: {len(rows)} 行 OK")

    for table, check, label in SPOT_CHECKS:
        if not check(data[table]):
            print(f"[中断] スポット検証 NG: {label} ({table})")
            return 1
        print(f"  スポット OK: {label}")

    # ---- 2. 接続と既存テーブル確認 ----
    import libsql_client

    client = libsql_client.create_client_sync(
        url=URL.replace("libsql://", "https://"), auth_token=TOKEN
    )
    print("\n=== 2. 既存テーブルの確認 ===")
    for table in TABLES:
        try:
            rs = client.execute(f"SELECT COUNT(*) FROM {table}")
            n = rs.rows[0][0]
            if n > 0 and not FORCE:
                print(f"[中断] {table} に既に {n} 行あります。作り直す場合は --force を付けて実行してください。")
                client.close()
                return 1
            print(f"  {table}: 既存 {n} 行" + (" → --force により再作成します" if FORCE else ""))
        except Exception:
            print(f"  {table}: 未作成 (新規作成します)")

    # ---- 3. 投入 (テーブル作成 → バッチ INSERT) ----
    print("\n=== 3. 投入 ===")
    for table, spec in TABLES.items():
        rows = data[table]
        client.execute(f"DROP TABLE IF EXISTS {table}")
        client.execute(spec["ddl"])
        cols = list(rows[0].keys())
        BATCH = 200
        total = 0
        for i in range(0, len(rows), BATCH):
            chunk = rows[i : i + BATCH]
            values = []
            for r in chunk:
                vals = []
                for c in cols:
                    v = r[c]
                    if c in spec["int_cols"] or c in spec["real_cols"]:
                        vals.append(str(v) if v not in ("", None) else "NULL")
                    else:
                        vals.append(sql_quote(v))
                values.append("(" + ",".join(vals) + ")")
            client.execute(
                f"INSERT INTO {table} ({','.join(cols)}) VALUES " + ",".join(values)
            )
            total += len(chunk)
        print(f"  {table}: {total} 行 投入完了")

    # ---- 4. 投入後の検証 (DB 側から読み直して確認) ----
    print("\n=== 4. 投入後の検証 (DB から読み直し) ===")
    ok = True
    checks = [
        ("SELECT COUNT(*) FROM cross_future_workforce", lambda v: 1800 <= v <= 1950, "全国市区町村の行数"),
        ("SELECT wa_decline_rate FROM cross_future_workforce WHERE municipality='大分市'", lambda v: abs(v + 14.1) < 0.2, "大分市 -14.1%"),
        ("SELECT scheduled_earnings FROM cross_wage_public WHERE prefecture='大分県' AND year_month='2025-12'", lambda v: v == 239448, "大分県 2025-12 給与"),
        ("SELECT min_wage_monthly_160h FROM cross_wage_public WHERE prefecture='大分県' AND year_month='2025-12'", lambda v: v == 165600, "大分県 最低賃金×160h"),
        ("SELECT job_change_desire_rate FROM cross_switcher_supply WHERE region_name='大分県'", lambda v: abs(v - 8.84) < 0.05, "大分県 転職希望 8.84%"),
        ("SELECT COUNT(*) FROM cross_switcher_supply", lambda v: 120 <= v <= 150, "134地域"),
    ]
    for sql, cond, label in checks:
        rs = client.execute(sql)
        v = rs.rows[0][0]
        good = cond(v)
        ok = ok and good
        print(f"  [{'OK' if good else 'NG'}] {label}: {v}")

    client.close()
    if ok:
        print("\n✅ 全テーブルの投入と検証が完了しました。")
        return 0
    print("\n⚠️ 投入は実行されましたが検証 NG があります。上の NG 行を確認してください。")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
