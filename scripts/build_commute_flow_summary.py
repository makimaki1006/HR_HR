# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 前提 (Step A): commute_flow_summary 生成スクリプト [JIS 版 / 2026-05-04 改訂]
============================================================================================

入力: `data/hellowork.db` の `v2_external_commute_od_with_codes` (JIS 5 桁 code 保持版、86,762 行)
      + `municipality_code_master` (1,917 行、area_type 含む)
出力:
  1. ローカル `data/hellowork.db` の `commute_flow_summary` テーブル (DROP + CREATE + INSERT)
     - 擬似コード版 (旧、27,879 行) を JIS 版で置換
  2. CSV `data/generated/commute_flow_summary.csv` (Turso 投入用)

集計仕様:
  - destination ごとに origin TOP 20
  - self-loop 除外 (origin_municipality_code == dest_municipality_code)
  - flow_share = origin flow_count / destination 総流入数
  - occupation_group_code = 'all'  (Step 5 後続で職業別に拡張)
  - occupation_group_name = '全職業'
  - estimated_target_flow_* は初期 NULL (Step 5 後続で計算)
  - estimation_method = 'commute_od_top20_all_occupation_jis'
  - destination_municipality_code / origin_municipality_code = **JIS 5 桁** (例: '01101' 札幌市中央区)

整合性ガード:
  - municipality_code_master と LEFT JOIN し、未登録コードがあれば fail (sys.exit(2))
  - 入力テーブル不在時も fail
  - origin_municipality_code / dest_municipality_code の NULL / 5 桁以外を SELECT で除外

使い方:
  python scripts/build_commute_flow_summary.py            # CSV + ローカル DB 投入
  python scripts/build_commute_flow_summary.py --dry-run  # CSV 出力のみ、DB 書き込みなし
  python scripts/build_commute_flow_summary.py --csv-only # 同上

設計原則:
  - **本番 (Turso) には書き込まない** (ユーザー手動 upload に委ねる)
  - 既存 `data/hellowork.db` 上の他テーブルには触らない (commute_flow_summary のみ DROP + CREATE)
  - **擬似コード版 (旧) は完全に置換**。`make_pseudo_code()` は削除済。
"""
import argparse
import csv
import sqlite3
import sys
import io
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DEFAULT_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
DEFAULT_CSV = SCRIPT_DIR.parent / "data" / "generated" / "commute_flow_summary.csv"

TOP_N = 20
ESTIMATION_METHOD = "commute_od_top20_all_occupation_jis"  # JIS 版判別用
DEFAULT_SOURCE_YEAR = 2020
SOURCE_TABLE = "v2_external_commute_od_with_codes"
MASTER_TABLE = "municipality_code_master"

# DDL: docs/survey_market_intelligence_phase0_2_schema.sql の同名テーブル定義と整合
DDL = """
CREATE TABLE IF NOT EXISTS commute_flow_summary (
    destination_municipality_code TEXT NOT NULL,
    destination_prefecture TEXT NOT NULL,
    destination_municipality_name TEXT NOT NULL,
    origin_municipality_code TEXT NOT NULL,
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    occupation_group_name TEXT NOT NULL DEFAULT '全職業',
    flow_count INTEGER NOT NULL DEFAULT 0,
    flow_share REAL,
    target_origin_population INTEGER,
    estimated_target_flow_conservative INTEGER,
    estimated_target_flow_standard INTEGER,
    estimated_target_flow_aggressive INTEGER,
    estimation_method TEXT,
    estimated_at TEXT,
    rank_to_destination INTEGER NOT NULL,
    source_year INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        destination_municipality_code,
        origin_municipality_code,
        occupation_group_code,
        source_year
    )
)
"""

# 出力カラム順 (CSV ヘッダーと INSERT で共通化)
COLUMNS = [
    "destination_municipality_code",
    "destination_prefecture",
    "destination_municipality_name",
    "origin_municipality_code",
    "origin_prefecture",
    "origin_municipality_name",
    "occupation_group_code",
    "occupation_group_name",
    "flow_count",
    "flow_share",
    "target_origin_population",
    "estimated_target_flow_conservative",
    "estimated_target_flow_standard",
    "estimated_target_flow_aggressive",
    "estimation_method",
    "estimated_at",
    "rank_to_destination",
    "source_year",
]


def assert_inputs_exist(conn: sqlite3.Connection):
    """入力テーブル v2_external_commute_od_with_codes と municipality_code_master の存在確認。"""
    for tbl in (SOURCE_TABLE, MASTER_TABLE):
        rows = conn.execute(
            f"SELECT name FROM sqlite_master WHERE type='table' AND name='{tbl}'"
        ).fetchall()
        if not rows:
            raise SystemExit(
                f"ERROR: 入力テーブル {tbl} が存在しません。前段の Worker A/B を完了してください。"
            )
        cnt = conn.execute(f"SELECT COUNT(*) FROM {tbl}").fetchone()[0]
        if cnt == 0:
            raise SystemExit(
                f"ERROR: 入力テーブル {tbl} が空です ({cnt} 行)。前段で投入を完了してください。"
            )


def assert_codes_in_master(conn: sqlite3.Connection):
    """v2_external_commute_od_with_codes に出現するすべての JIS コードが
    municipality_code_master に登録されているか LEFT JOIN で検証。

    未登録コードが検出されれば fail (Step 5 着手前の整合性ガード)。

    e-Stat 特殊コード (`99998` 外国 / `99999` 不詳) は本来の市区町村ではないため
    fetch_clean_commute_od の SELECT で除外し、本検証では skip 対象になる。
    """
    cur = conn.cursor()
    # origin 側 (e-Stat 特殊コード 99xxx は除外、それ以外で master 未登録があれば fail)
    sql_origin = f"""
    SELECT cod.origin_municipality_code,
           cod.origin_prefecture,
           cod.origin_municipality_name,
           COUNT(*) AS occurrences
    FROM {SOURCE_TABLE} AS cod
    LEFT JOIN {MASTER_TABLE} AS mcm
      ON mcm.municipality_code = cod.origin_municipality_code
    WHERE mcm.municipality_code IS NULL
      AND SUBSTR(cod.origin_municipality_code, 1, 2) <= '47'   -- 99xxx (不詳/外国) を除外
    GROUP BY cod.origin_municipality_code,
             cod.origin_prefecture,
             cod.origin_municipality_name
    ORDER BY occurrences DESC
    """
    sql_dest = f"""
    SELECT cod.dest_municipality_code,
           cod.dest_prefecture,
           cod.dest_municipality_name,
           COUNT(*) AS occurrences
    FROM {SOURCE_TABLE} AS cod
    LEFT JOIN {MASTER_TABLE} AS mcm
      ON mcm.municipality_code = cod.dest_municipality_code
    WHERE mcm.municipality_code IS NULL
      AND SUBSTR(cod.dest_municipality_code, 1, 2) <= '47'   -- 99xxx (不詳/外国) を除外
    GROUP BY cod.dest_municipality_code,
             cod.dest_prefecture,
             cod.dest_municipality_name
    ORDER BY occurrences DESC
    """
    missing_origin = cur.execute(sql_origin).fetchall()
    missing_dest = cur.execute(sql_dest).fetchall()
    if missing_origin or missing_dest:
        print("ERROR: master 未登録コードを検出 (Phase 3 整合性ガード)")
        if missing_origin:
            print(f"  origin 側 ({len(missing_origin)} 件):")
            for r in missing_origin[:10]:
                print(f"    {r}")
            if len(missing_origin) > 10:
                print(f"    ... 他 {len(missing_origin) - 10} 件")
        if missing_dest:
            print(f"  dest 側 ({len(missing_dest)} 件):")
            for r in missing_dest[:10]:
                print(f"    {r}")
            if len(missing_dest) > 10:
                print(f"    ... 他 {len(missing_dest) - 10} 件")
        raise SystemExit(
            "→ municipality_code_master の整備が不完全です。build_municipality_code_master.py を再実行するか、"
            "DESIGNATED_CITY_AGGREGATE_CODES に追加が必要な可能性があります。"
        )
    print("  ✅ master 突合 OK (origin/dest すべて master に登録済)")


def fetch_clean_commute_od(conn: sqlite3.Connection):
    """v2_external_commute_od_with_codes から JIS code ベースで取得。

    - origin/dest_municipality_code の NULL / 5 桁以外は SELECT で除外
    - e-Stat 特殊コード (`99999` 従業地・通学地「不詳」、`99998` 「不詳・外国」) を除外
      → pref_code (上位 2 桁) が 01〜47 でないものを除外
    - self-loop は origin_code == dest_code で判定
    - total_commuters > 0 のみ
    """
    sql = f"""
    SELECT origin_municipality_code, origin_prefecture, origin_municipality_name,
           dest_municipality_code,   dest_prefecture,   dest_municipality_name,
           total_commuters, male_commuters, female_commuters, reference_year
    FROM {SOURCE_TABLE}
    WHERE origin_municipality_code IS NOT NULL
      AND dest_municipality_code   IS NOT NULL
      AND LENGTH(origin_municipality_code) = 5
      AND LENGTH(dest_municipality_code)   = 5
      AND SUBSTR(origin_municipality_code, 1, 2) <= '47'   -- e-Stat 特殊 99xxx 除外 (不詳/外国)
      AND SUBSTR(dest_municipality_code,   1, 2) <= '47'   -- 同上
      AND origin_municipality_code != dest_municipality_code  -- self-loop 除外
      AND total_commuters IS NOT NULL AND total_commuters > 0
    """
    return conn.execute(sql).fetchall()


def build_summary_rows(raw_rows):
    """destination ごとに TOP N を抽出し flow_share を計算 → list[dict]。

    入力は (origin_code, origin_pref, origin_muni, dest_code, dest_pref, dest_muni,
            total, male, female, year) のタプル。
    """
    by_dest = defaultdict(list)
    for (oc, op, om, dc, dp, dm, total, male, female, year) in raw_rows:
        # キーは JIS code ベース (擬似コード時代との非互換)
        by_dest[(dc, dp, dm, year or DEFAULT_SOURCE_YEAR)].append((oc, op, om, total))

    estimated_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
    out = []
    for (dest_code, dp, dm, year), origins in by_dest.items():
        # destination 総流入数 (TOP N で打ち切る前の合計)
        dest_total = sum(c for _, _, _, c in origins)
        if dest_total <= 0:
            continue
        # TOP N (count 降順)
        origins.sort(key=lambda x: x[3], reverse=True)
        for rank, (origin_code, op, om, count) in enumerate(origins[:TOP_N], start=1):
            out.append(
                {
                    "destination_municipality_code": dest_code,    # JIS 5 桁
                    "destination_prefecture": dp,
                    "destination_municipality_name": dm,
                    "origin_municipality_code": origin_code,       # JIS 5 桁
                    "origin_prefecture": op,
                    "origin_municipality_name": om,
                    "occupation_group_code": "all",
                    "occupation_group_name": "全職業",
                    "flow_count": count,
                    "flow_share": round(count / dest_total, 6),
                    "target_origin_population": None,
                    "estimated_target_flow_conservative": None,
                    "estimated_target_flow_standard": None,
                    "estimated_target_flow_aggressive": None,
                    "estimation_method": ESTIMATION_METHOD,
                    "estimated_at": estimated_at,
                    "rank_to_destination": rank,
                    "source_year": year,
                }
            )
    return out


def write_csv(rows, csv_path: Path):
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


def write_db(rows, conn: sqlite3.Connection):
    """commute_flow_summary を DROP + CREATE + INSERT。他テーブルには触らない。"""
    conn.execute("DROP TABLE IF EXISTS commute_flow_summary")
    conn.execute(DDL)
    insert_sql = f"INSERT INTO commute_flow_summary ({', '.join(COLUMNS)}) VALUES ({', '.join('?' * len(COLUMNS))})"
    conn.executemany(insert_sql, [tuple(r[c] for c in COLUMNS) for r in rows])
    conn.commit()


def verify(conn: sqlite3.Connection):
    """検証 SQL: 行数 / rank 重複 / rank 範囲 / self-loop / share 範囲 / 日本語表示。"""
    print("\n--- 検証 SQL ---")
    cur = conn.cursor()

    # 1. 行数
    n = cur.execute("SELECT COUNT(*) FROM commute_flow_summary").fetchone()[0]
    print(f"  [1] 行数: {n:,}")

    # 2. destination ごとの rank 重複なし
    dup = cur.execute(
        """
        SELECT destination_municipality_code, occupation_group_code, source_year, rank_to_destination, COUNT(*) AS c
        FROM commute_flow_summary
        GROUP BY destination_municipality_code, occupation_group_code, source_year, rank_to_destination
        HAVING c > 1
        """
    ).fetchall()
    if dup:
        print(f"  [2] ❌ rank 重複あり: {len(dup)} 件")
    else:
        print(f"  [2] ✅ rank 重複なし")

    # 3. rank が 1〜TOP_N
    out_of_range = cur.execute(
        f"SELECT COUNT(*) FROM commute_flow_summary WHERE rank_to_destination < 1 OR rank_to_destination > {TOP_N}"
    ).fetchone()[0]
    print(f"  [3] {'✅' if out_of_range == 0 else '❌'} rank 範囲外: {out_of_range} 件")

    # 4. self-loop なし
    self_loop = cur.execute(
        """
        SELECT COUNT(*) FROM commute_flow_summary
        WHERE destination_prefecture = origin_prefecture
          AND destination_municipality_name = origin_municipality_name
        """
    ).fetchone()[0]
    print(f"  [4] {'✅' if self_loop == 0 else '❌'} self-loop: {self_loop} 件")

    # 5. flow_share が 0〜1
    bad_share = cur.execute(
        "SELECT COUNT(*) FROM commute_flow_summary WHERE flow_share < 0 OR flow_share > 1.0"
    ).fetchone()[0]
    print(f"  [5] {'✅' if bad_share == 0 else '❌'} flow_share 範囲外: {bad_share} 件")

    # 6. flow_share の合計が destination ごとに 1 以下 (TOP N で打ち切るため <= 1)
    over_total = cur.execute(
        """
        SELECT destination_municipality_code, SUM(flow_share) AS total
        FROM commute_flow_summary
        GROUP BY destination_municipality_code
        HAVING total > 1.0001
        """
    ).fetchall()
    print(f"  [6] {'✅' if not over_total else '❌'} destination 別 flow_share 合計 > 1.0: {len(over_total)} 件")

    # 7. 日本語表示正常 (DISTINCT prefecture サンプル)
    samples = cur.execute(
        "SELECT DISTINCT destination_prefecture FROM commute_flow_summary ORDER BY destination_prefecture LIMIT 8"
    ).fetchall()
    print(f"  [7] DISTINCT destination_prefecture サンプル: {[s[0] for s in samples]}")

    # 8. destination 数
    dest_count = cur.execute(
        "SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary"
    ).fetchone()[0]
    print(f"  [8] DISTINCT destination 数: {dest_count:,}")

    # 9. occupation_group の確認
    occ = cur.execute(
        "SELECT occupation_group_code, occupation_group_name, COUNT(*) FROM commute_flow_summary GROUP BY occupation_group_code"
    ).fetchall()
    print(f"  [9] occupation_group: {occ}")

    # 10. estimated_target_flow_* がすべて NULL であること (初期値仕様通り)
    non_null = cur.execute(
        """
        SELECT COUNT(*) FROM commute_flow_summary
        WHERE estimated_target_flow_conservative IS NOT NULL
           OR estimated_target_flow_standard IS NOT NULL
           OR estimated_target_flow_aggressive IS NOT NULL
        """
    ).fetchone()[0]
    print(f"  [10] {'✅' if non_null == 0 else '⚠️'} estimated_target_flow_* 非 NULL: {non_null} 件 (期待: 0)")

    # 11. JIS 版固有: コード形式 (5 桁数字)
    non_jis = cur.execute(
        """
        SELECT COUNT(*) FROM commute_flow_summary
        WHERE LENGTH(destination_municipality_code) != 5
           OR LENGTH(origin_municipality_code) != 5
           OR destination_municipality_code GLOB '*[^0-9]*'
           OR origin_municipality_code GLOB '*[^0-9]*'
        """
    ).fetchone()[0]
    print(f"  [11] {'✅' if non_jis == 0 else '❌'} 非 JIS code (5 桁数字以外): {non_jis} 件 (期待: 0)")

    # 12. master 突合: 全 code が master に存在
    not_in_master = cur.execute(
        f"""
        SELECT COUNT(*) FROM commute_flow_summary AS cfs
        LEFT JOIN {MASTER_TABLE} AS dst ON dst.municipality_code = cfs.destination_municipality_code
        LEFT JOIN {MASTER_TABLE} AS org ON org.municipality_code = cfs.origin_municipality_code
        WHERE dst.municipality_code IS NULL OR org.municipality_code IS NULL
        """
    ).fetchone()[0]
    print(f"  [12] {'✅' if not_in_master == 0 else '❌'} master 未登録 code を持つ行: {not_in_master} 件 (期待: 0)")

    # 13. 重要 4 コードの destination としての出現
    print(f"  [13] 重要 4 コードの destination 出現件数:")
    for code, label in [('13100', '特別区部'), ('13101', '千代田区'), ('01100', '札幌市'), ('01101', '札幌市中央区')]:
        c = cur.execute(
            "SELECT COUNT(*) FROM commute_flow_summary WHERE destination_municipality_code = ?",
            (code,)
        ).fetchone()[0]
        marker = "✅" if c > 0 else "⚠️"
        print(f"        {marker} {code} {label}: {c} 件 (TOP {TOP_N} 上限)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1] if __doc__ else "")
    parser.add_argument("--db", type=Path, default=DEFAULT_DB, help=f"ローカル sqlite DB (default: {DEFAULT_DB})")
    parser.add_argument("--csv", type=Path, default=DEFAULT_CSV, help=f"CSV 出力先 (default: {DEFAULT_CSV})")
    parser.add_argument("--dry-run", action="store_true", help="CSV のみ出力、DB 書き込みなし")
    parser.add_argument("--csv-only", action="store_true", help="--dry-run のエイリアス")
    args = parser.parse_args()

    print("=" * 70)
    print("Phase 3 Step A: commute_flow_summary 生成 [JIS 版] (READ-only on Turso)")
    print("=" * 70)

    if not args.db.exists():
        print(f"ERROR: DB が見つかりません: {args.db}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(str(args.db))

    # 入力テーブル存在 + 行数確認 (Worker A/B 完了が前提)
    assert_inputs_exist(conn)
    cnt_src = conn.execute(f"SELECT COUNT(*) FROM {SOURCE_TABLE}").fetchone()[0]
    cnt_master = conn.execute(f"SELECT COUNT(*) FROM {MASTER_TABLE}").fetchone()[0]
    print(f"\n入力: {SOURCE_TABLE} = {cnt_src:,} 行")
    print(f"入力: {MASTER_TABLE} = {cnt_master:,} 行")

    # master 整合性ガード (未登録コード検出時は fail)
    print("\nmaster 突合 (未登録コード検出時は fail)...")
    assert_codes_in_master(conn)

    # 集計
    print("\n集計中...")
    raw = fetch_clean_commute_od(conn)
    print(f"  JIS code 5 桁ガード + self-loop 除外 後: {len(raw):,} 行")

    rows = build_summary_rows(raw)
    print(f"  集計後 (TOP {TOP_N} × destination): {len(rows):,} 行")

    # CSV 出力
    write_csv(rows, args.csv)
    print(f"\nCSV 出力: {args.csv} ({args.csv.stat().st_size:,} B)")

    # DB 書き込み (dry-run でなければ)
    dry_run = args.dry_run or args.csv_only
    if dry_run:
        print("\n[--dry-run / --csv-only] DB 書き込みスキップ")
    else:
        write_db(rows, conn)
        print(f"\nDB 書き込み: {args.db}::commute_flow_summary (DROP + CREATE + INSERT)")
        # 検証は DB に書いた場合のみ
        verify(conn)

    conn.close()
    print("\n完了。次のステップ:")
    print("  1. ユーザー手動: scripts/upload_to_turso.py の TABLES に commute_flow_summary 追加")
    print("  2. ユーザー手動: python scripts/upload_to_turso.py で Turso 反映")
    print("  3. python scripts/verify_turso_v2_sync.py で MATCH 確認")
    return 0


if __name__ == "__main__":
    sys.exit(main())
