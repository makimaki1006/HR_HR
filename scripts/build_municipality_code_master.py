# -*- coding: utf-8 -*-
"""
Phase 3 JIS 整備 (Worker B2): municipality_code_master 派生スクリプト
====================================================================

入力: ローカル `data/hellowork.db` の `v2_external_commute_od_with_codes`
      (Worker A 改修 + e-Stat 再 fetch 完了後に投入される)

出力:
  1. ローカル `data/hellowork.db` の `municipality_code_master` テーブル
     (DROP + CREATE + INSERT、area_type / area_level / parent_code 含む)
  2. CSV `data/generated/municipality_code_master.csv` (Turso 投入用)

設計仕様: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md (Worker B1)

使い方:
  python scripts/build_municipality_code_master.py            # 通常実行
  python scripts/build_municipality_code_master.py --dry-run  # CREATE TABLE のみ実行、INSERT スキップ
  python scripts/build_municipality_code_master.py --csv-only # CSV のみ生成、DB 書き込みなし

設計原則:
  - 入力テーブル空でも dry-run 可能 (CREATE + 0 INSERT で正常終了)
  - Turso には書き込まない (Claude/AI の禁止事項遵守)
  - 既存テーブルへの破壊変更なし (municipality_code_master は新規テーブル)
"""
import argparse
import csv
import sqlite3
import sys
import io
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DEFAULT_DB = SCRIPT_DIR.parent / "data" / "hellowork.db"
DEFAULT_CSV = SCRIPT_DIR.parent / "data" / "generated" / "municipality_code_master.csv"

SOURCE_TABLE = "v2_external_commute_od_with_codes"
TARGET_TABLE = "municipality_code_master"
DEFAULT_SOURCE_YEAR = 2020
DEFAULT_SOURCE = "estat_commute_od"

# DDL は docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md §2.2 と完全整合
DDL = """
CREATE TABLE IF NOT EXISTS municipality_code_master (
    municipality_code TEXT PRIMARY KEY,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    pref_code TEXT NOT NULL,
    area_type TEXT NOT NULL CHECK (area_type IN (
        'municipality',
        'designated_ward',
        'special_ward',
        'aggregate_city',
        'aggregate_special_wards'
    )),
    area_level TEXT NOT NULL CHECK (area_level IN ('unit', 'aggregate')),
    is_special_ward INTEGER NOT NULL DEFAULT 0,
    is_designated_ward INTEGER NOT NULL DEFAULT 0,
    parent_code TEXT,
    source TEXT NOT NULL DEFAULT 'estat_commute_od',
    source_year INTEGER NOT NULL DEFAULT 2020,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_mcm_pref_muni
    ON municipality_code_master (prefecture, municipality_name);
CREATE INDEX IF NOT EXISTS idx_mcm_pref_code
    ON municipality_code_master (pref_code);
CREATE INDEX IF NOT EXISTS idx_mcm_area_type
    ON municipality_code_master (area_type);
CREATE INDEX IF NOT EXISTS idx_mcm_area_level
    ON municipality_code_master (area_level);
CREATE INDEX IF NOT EXISTS idx_mcm_parent
    ON municipality_code_master (parent_code);
"""

COLUMNS = [
    "municipality_code",
    "prefecture",
    "municipality_name",
    "pref_code",
    "area_type",
    "area_level",
    "is_special_ward",
    "is_designated_ward",
    "parent_code",
    "source",
    "source_year",
]


# 政令指定都市 (全国 20 市) の集約コード ハードコードマスタ (2026-05-04 Worker A 調査結果)
# 既存 15 市 (suffix=100) + 追加 5 市 (suffix=130/140/150)
# Worker A 調査の根拠: v2_external_commute_od_with_codes の DISTINCT で確定
DESIGNATED_CITY_AGGREGATE_CODES = {
    # suffix=100 (15 市)
    "01100": "札幌市",
    "04100": "仙台市",
    "11100": "さいたま市",
    "12100": "千葉市",
    "14100": "横浜市",
    "15100": "新潟市",
    "22100": "静岡市",
    "23100": "名古屋市",
    "26100": "京都市",
    "27100": "大阪市",
    "28100": "神戸市",
    "33100": "岡山市",
    "34100": "広島市",
    "40100": "北九州市",
    "43100": "熊本市",
    # 追加 5 市 (suffix=130/140/150) - Worker A 調査結果
    "14130": "川崎市",
    "14150": "相模原市",
    "22130": "浜松市",
    "27140": "堺市",
    "40130": "福岡市",
}

# 政令市名 → 集約コードの逆引き (designated_ward の parent_code 算出用)
# 順序固定: 市名の startswith 判定で「札幌市」と「札幌市中央区」を区別するため、
# 文字列長の長い順にソート (全 20 市は等長 3〜5 文字なので影響軽微)
DESIGNATED_CITY_NAME_TO_CODE = {
    name: code for code, name in DESIGNATED_CITY_AGGREGATE_CODES.items()
}
# 名前 prefix 判定用に長い順
_DC_NAMES_BY_LEN = sorted(DESIGNATED_CITY_NAME_TO_CODE.keys(), key=len, reverse=True)


def derive_area_type(code: str, prefecture: str, municipality_name: str) -> tuple[str, str, str | None]:
    """5 桁 code + 名称から area_type / area_level / parent_code を派生。

    docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md §2.5 の判定ロジックを実装。
    2026-05-04 改訂: 政令市本体の suffix が '100' 以外 (川崎=14130 等) のケースに対応するため、
                     ハードコード DESIGNATED_CITY_AGGREGATE_CODES を導入。
    """
    if len(code) != 5 or not code.isdigit():
        # 不正な code は municipality 扱い (フォールバック)
        return "municipality", "unit", None

    pref_code = code[:2]
    suffix = code[2:5]

    # 1. 特別区部 (13100)
    if code == "13100":
        return "aggregate_special_wards", "aggregate", None

    # 2. 政令市本体 (ハードコード 20 市)
    #    suffix='100' の単純判定では川崎/相模原/浜松/堺/福岡を取りこぼすため、
    #    確定マスタで判定する。
    if code in DESIGNATED_CITY_AGGREGATE_CODES:
        return "aggregate_city", "aggregate", None

    # 3. 特別区: 13101〜13123
    if pref_code == "13" and "101" <= suffix <= "123":
        return "special_ward", "unit", "13100"  # 親 = 特別区部

    # 4. 政令市の区: pref != 13 かつ name に "市" を含み末尾 "区"
    #    parent_code は名称 prefix から逆引き (e.g. "川崎市川崎区" → "14130")
    if pref_code != "13" and "市" in municipality_name and municipality_name.endswith("区"):
        for city_name in _DC_NAMES_BY_LEN:
            if municipality_name.startswith(city_name):
                parent = DESIGNATED_CITY_NAME_TO_CODE[city_name]
                # 親 code の pref_code が一致する場合のみ採用 (異 pref 衝突防御)
                if parent[:2] == pref_code:
                    return "designated_ward", "unit", parent
        # 逆引き失敗 (理論上発生しないはずだが) → parent_code 不明で designated_ward
        return "designated_ward", "unit", None

    # 5. 一般市町村
    return "municipality", "unit", None


def fetch_distinct_municipalities(conn):
    """v2_external_commute_od_with_codes から origin/dest 両側で DISTINCT 抽出。

    入力テーブル空なら空リスト返却 (dry-run でも安全動作)。
    """
    # テーブル存在確認
    rows = conn.execute(
        f"SELECT name FROM sqlite_master WHERE type='table' AND name='{SOURCE_TABLE}'"
    ).fetchall()
    if not rows:
        print(f"  ⚠️  入力テーブル {SOURCE_TABLE} が存在しません (Worker A 改修 + e-Stat 再 fetch が前提)")
        return []

    cnt = conn.execute(f"SELECT COUNT(*) FROM {SOURCE_TABLE}").fetchone()[0]
    print(f"  入力 {SOURCE_TABLE}: {cnt:,} 行")
    if cnt == 0:
        print(f"  → 空テーブルのため抽出スキップ (CREATE のみ実行可)")
        return []

    sql = f"""
    SELECT
        origin_municipality_code AS code,
        origin_prefecture AS prefecture,
        origin_municipality_name AS municipality_name
    FROM {SOURCE_TABLE}
    WHERE origin_municipality_code IS NOT NULL
      AND origin_municipality_code <> ''

    UNION

    SELECT
        dest_municipality_code AS code,
        dest_prefecture AS prefecture,
        dest_municipality_name AS municipality_name
    FROM {SOURCE_TABLE}
    WHERE dest_municipality_code IS NOT NULL
      AND dest_municipality_code <> ''
    """
    distinct_rows = conn.execute(sql).fetchall()
    return distinct_rows


def build_master_rows(distinct_rows):
    """DISTINCT (code, pref, muni) → master の dict list 構築。area_type 等を派生。"""
    out = []
    for code, pref, muni in distinct_rows:
        if not code or not pref or not muni:
            continue
        area_type, area_level, parent_code = derive_area_type(code, pref, muni)
        is_special = 1 if area_type == "special_ward" else 0
        is_designated = 1 if area_type == "designated_ward" else 0
        out.append(
            {
                "municipality_code": code,
                "prefecture": pref,
                "municipality_name": muni,
                "pref_code": code[:2],
                "area_type": area_type,
                "area_level": area_level,
                "is_special_ward": is_special,
                "is_designated_ward": is_designated,
                "parent_code": parent_code,
                "source": DEFAULT_SOURCE,
                "source_year": DEFAULT_SOURCE_YEAR,
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
    """DROP + CREATE + INSERT。他テーブルには触らない。"""
    conn.execute(f"DROP TABLE IF EXISTS {TARGET_TABLE}")
    conn.executescript(DDL)
    if not rows:
        conn.commit()
        return 0
    insert_sql = (
        f"INSERT INTO {TARGET_TABLE} ({', '.join(COLUMNS)}) "
        f"VALUES ({', '.join('?' * len(COLUMNS))})"
    )
    conn.executemany(insert_sql, [tuple(r[c] for c in COLUMNS) for r in rows])
    conn.commit()
    return len(rows)


def verify(conn: sqlite3.Connection):
    """検証 SQL: 行数 / area_type 分布 / 47 都道府県カバレッジ / PK 一意性 / 親子整合"""
    print("\n--- 検証 SQL ---")
    cur = conn.cursor()

    cnt = cur.execute(f"SELECT COUNT(*) FROM {TARGET_TABLE}").fetchone()[0]
    print(f"  [1] 行数: {cnt:,}")
    if cnt == 0:
        print("  → 入力空のため検証スキップ")
        return

    dup = cur.execute(
        f"SELECT COUNT(*) - COUNT(DISTINCT municipality_code) FROM {TARGET_TABLE}"
    ).fetchone()[0]
    print(f"  [2] PK 重複: {dup} 件 (期待: 0)")

    pref_count = cur.execute(
        f"SELECT COUNT(DISTINCT pref_code) FROM {TARGET_TABLE}"
    ).fetchone()[0]
    print(f"  [3] DISTINCT pref_code: {pref_count} (期待: 47)")

    print(f"  [4] area_type 分布:")
    rows = cur.execute(
        f"SELECT area_type, area_level, COUNT(*) FROM {TARGET_TABLE} "
        "GROUP BY area_type, area_level ORDER BY 1, 2"
    ).fetchall()
    for at, al, c in rows:
        print(f"      {at}/{al}: {c:,}")

    sw = cur.execute(
        f"SELECT COUNT(*) FROM {TARGET_TABLE} WHERE area_type = 'special_ward'"
    ).fetchone()[0]
    print(f"  [5] 特別区: {sw} (期待: 23)")

    aggr_sw = cur.execute(
        f"SELECT COUNT(*) FROM {TARGET_TABLE} WHERE area_type = 'aggregate_special_wards'"
    ).fetchone()[0]
    print(f"  [6] 特別区部 (集約): {aggr_sw} (期待: 0 or 1)")

    aggr_city = cur.execute(
        f"SELECT COUNT(*) FROM {TARGET_TABLE} WHERE area_type = 'aggregate_city'"
    ).fetchone()[0]
    print(f"  [7] 政令市本体 (集約): {aggr_city} (期待: 約 20)")

    # 親子整合: parent_code を持つ子は area_level='unit'、parent_code が指す親は area_level='aggregate'
    orphan = cur.execute(
        f"SELECT COUNT(*) FROM {TARGET_TABLE} WHERE parent_code IS NOT NULL "
        f"AND parent_code NOT IN (SELECT municipality_code FROM {TARGET_TABLE})"
    ).fetchone()[0]
    print(f"  [8] 孤児 parent_code (親不在): {orphan} (期待: 0、ただし入力に親集約コードがなければ非ゼロ可)")

    # サンプル
    print(f"  [9] サンプル各 area_type の代表 1 件:")
    for at in ["municipality", "designated_ward", "special_ward", "aggregate_city", "aggregate_special_wards"]:
        s = cur.execute(
            f"SELECT municipality_code, prefecture, municipality_name, parent_code "
            f"FROM {TARGET_TABLE} WHERE area_type = ? LIMIT 1",
            (at,),
        ).fetchone()
        print(f"      {at}: {s}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1] if __doc__ else "")
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    parser.add_argument("--csv", type=Path, default=DEFAULT_CSV)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="CREATE TABLE のみ実行、INSERT スキップ (入力テーブル空でも実行可)",
    )
    parser.add_argument(
        "--csv-only", action="store_true", help="CSV のみ生成、DB 書き込みなし"
    )
    args = parser.parse_args()

    print("=" * 70)
    print("Phase 3 Worker B2: municipality_code_master 生成 (READ-only on Turso)")
    print("=" * 70)

    if not args.db.exists():
        print(f"ERROR: DB が見つかりません: {args.db}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(str(args.db))

    # CREATE TABLE は常に実行 (冪等)
    print(f"\nCREATE TABLE {TARGET_TABLE} (冪等)")
    conn.executescript(DDL)
    conn.commit()

    if args.dry_run:
        cnt = conn.execute(f"SELECT COUNT(*) FROM {TARGET_TABLE}").fetchone()[0]
        print(f"\n[--dry-run] CREATE のみ実行。{TARGET_TABLE} 現在の行数: {cnt:,}")
        print(f"  入力テーブル {SOURCE_TABLE} の状態確認:")
        rows = conn.execute(
            f"SELECT name FROM sqlite_master WHERE type='table' AND name='{SOURCE_TABLE}'"
        ).fetchall()
        if not rows:
            print(f"  ⚠️  {SOURCE_TABLE} 不在 (Worker A 改修 + e-Stat 再 fetch が前提)")
        else:
            src_cnt = conn.execute(f"SELECT COUNT(*) FROM {SOURCE_TABLE}").fetchone()[0]
            print(f"  ✅ {SOURCE_TABLE}: {src_cnt:,} 行")
        conn.close()
        print(f"\nDry-run 完了。実投入する場合は --dry-run を外してください。")
        return 0

    # 入力抽出
    distinct_rows = fetch_distinct_municipalities(conn)
    print(f"  DISTINCT 自治体: {len(distinct_rows):,}")

    # area_type 派生
    rows = build_master_rows(distinct_rows)
    print(f"  master 行 (area_type 派生済): {len(rows):,}")

    # CSV 出力
    write_csv(rows, args.csv)
    print(f"\nCSV 出力: {args.csv} ({args.csv.stat().st_size:,} B)")

    # DB 書き込み
    if args.csv_only:
        print("\n[--csv-only] DB 書き込みスキップ")
    else:
        n = write_db(rows, conn)
        print(f"\nDB 書き込み: {args.db}::{TARGET_TABLE} ({n:,} 行 INSERT)")
        verify(conn)

    conn.close()
    print("\n完了。次のステップ:")
    print(f"  1. ユーザー手動: Worker A の --with-codes で {SOURCE_TABLE} を投入")
    print(f"  2. ユーザー手動: 本スクリプトを再実行")
    print(f"  3. Turso 反映 (upload_to_turso.py 改修 + 実行)")
    print(f"  4. Worker C: build_commute_flow_summary.py JIS 化")
    return 0


if __name__ == "__main__":
    sys.exit(main())
