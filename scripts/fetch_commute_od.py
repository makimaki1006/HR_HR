#!/usr/bin/env python3
"""
通勤OD（起終点）データ取得スクリプト
e-Stat API から令和2年国勢調査 従業地・通学地集計を取得し、
市区町村間の通勤フロー行列をSQLiteに保存する。

statsDataId: 0003454527
  area = 常住地（住んでいる場所）
  cat02 = 従業地（働いている場所）
  cat01 = 性別（0=総数, 1=男, 2=女）
"""

import json
import os
import sqlite3
import sys
import time
import urllib.request
import urllib.error

APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0003454527"
API_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
MIN_COMMUTERS = 10  # 10人未満のフローは除外（ノイズ削減）
DB_PATH = os.path.join(os.path.dirname(__file__), "..", "data", "hellowork.db")

# 都道府県コード（先頭2桁）
PREF_CODES = [f"{i:02d}" for i in range(1, 48)]
PREF_NAMES = {
    "01": "北海道", "02": "青森県", "03": "岩手県", "04": "宮城県", "05": "秋田県",
    "06": "山形県", "07": "福島県", "08": "茨城県", "09": "栃木県", "10": "群馬県",
    "11": "埼玉県", "12": "千葉県", "13": "東京都", "14": "神奈川県", "15": "新潟県",
    "16": "富山県", "17": "石川県", "18": "福井県", "19": "山梨県", "20": "長野県",
    "21": "岐阜県", "22": "静岡県", "23": "愛知県", "24": "三重県", "25": "滋賀県",
    "26": "京都府", "27": "大阪府", "28": "兵庫県", "29": "奈良県", "30": "和歌山県",
    "31": "鳥取県", "32": "島根県", "33": "岡山県", "34": "広島県", "35": "山口県",
    "36": "徳島県", "37": "香川県", "38": "愛媛県", "39": "高知県", "40": "福岡県",
    "41": "佐賀県", "42": "長崎県", "43": "熊本県", "44": "大分県", "45": "宮崎県",
    "46": "鹿児島県", "47": "沖縄県",
}


def create_tables(conn):
    """通勤ODテーブルと集約テーブルを作成"""
    conn.executescript("""
        CREATE TABLE IF NOT EXISTS v2_external_commute_od (
            origin_pref TEXT NOT NULL,
            origin_muni TEXT NOT NULL,
            dest_pref TEXT NOT NULL,
            dest_muni TEXT NOT NULL,
            total_commuters INTEGER NOT NULL,
            male_commuters INTEGER DEFAULT 0,
            female_commuters INTEGER DEFAULT 0,
            reference_year INTEGER DEFAULT 2020,
            PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
        );
        CREATE INDEX IF NOT EXISTS idx_cod_dest ON v2_external_commute_od(dest_pref, dest_muni);
        CREATE INDEX IF NOT EXISTS idx_cod_origin ON v2_external_commute_od(origin_pref, origin_muni);

        CREATE TABLE IF NOT EXISTS v2_commute_flow_summary (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            direction TEXT NOT NULL,
            total_commuters INTEGER NOT NULL,
            self_commute_count INTEGER DEFAULT 0,
            self_commute_rate REAL DEFAULT 0,
            partner_count INTEGER DEFAULT 0,
            top10_json TEXT NOT NULL DEFAULT '[]',
            PRIMARY KEY (prefecture, municipality, direction)
        );
    """)


def fetch_area_names(app_id):
    """エリアコード→市区町村名のマッピングを取得"""
    url = f"{API_BASE}?appId={app_id}&statsDataId={STATS_DATA_ID}&limit=1"
    with urllib.request.urlopen(url, timeout=30) as resp:
        d = json.load(resp)

    classes = d["GET_STATS_DATA"]["STATISTICAL_DATA"]["CLASS_INF"]["CLASS_OBJ"]
    area_map = {}
    cat02_map = {}
    for c in classes:
        if c["@id"] == "area":
            for a in c["CLASS"]:
                area_map[a["@code"]] = a["@name"]
        elif c["@id"] == "cat02":
            for a in c["CLASS"]:
                cat02_map[a["@code"]] = a["@name"]

    return area_map, cat02_map


def code_to_pref_muni(code, name_map):
    """5桁コードから都道府県名と市区町村名を分離"""
    name = name_map.get(code, "")
    pref_code = code[:2]
    pref_name = PREF_NAMES.get(pref_code, "")

    if code.endswith("000"):
        # 都道府県レベル
        return pref_name, ""

    # 市区町村名 = 全体名から都道府県名を除去
    muni = name.replace(pref_name, "").strip()
    return pref_name, muni


def fetch_pref_data(app_id, pref_code, area_map, cat02_map):
    """1都道府県分のODデータを取得（ページング対応）"""
    # 常住地=この都道府県の市区町村、従業地=全国
    # areaフィルタ: この都道府県のコードで始まるエリア
    area_codes = [c for c in area_map.keys()
                  if c.startswith(pref_code) and not c.endswith("000")]

    if not area_codes:
        return []

    # エリアコードが多い場合はチャンク分割（API制限対策: 北海道等）
    CHUNK_SIZE = 50
    area_chunks = [area_codes[i:i+CHUNK_SIZE] for i in range(0, len(area_codes), CHUNK_SIZE)]

    results = []
    for sex_code, sex_label in [("0", "total"), ("1", "male"), ("2", "female")]:
      for chunk in area_chunks:
        page = 1
        while True:
            area_str = ",".join(chunk)
            url = (f"{API_BASE}?appId={app_id}&statsDataId={STATS_DATA_ID}"
                   f"&cdArea={area_str}&cdCat01={sex_code}"
                   f"&limit=100000&startPosition={(page-1)*100000+1}")

            d = None
            for retry in range(3):
                try:
                    with urllib.request.urlopen(url, timeout=120) as resp:
                        d = json.load(resp)
                    break
                except (urllib.error.URLError, json.JSONDecodeError, TimeoutError, OSError, Exception) as e:
                    print(f"    Retry {retry+1}/3 (pref={pref_code}, sex={sex_code}): {e}")
                    time.sleep(5 * (retry + 1))
            if d is None:
                print(f"    SKIP (pref={pref_code}, sex={sex_code}): all retries failed")
                break

            status = d["GET_STATS_DATA"]["RESULT"]["STATUS"]
            if status != 0:
                print(f"    API status {status} for pref={pref_code}")
                break

            values = d["GET_STATS_DATA"]["STATISTICAL_DATA"]["DATA_INF"].get("VALUE", [])
            if not values:
                break

            for v in values:
                origin_code = v.get("@area", "")
                dest_code = v.get("@cat02", "")
                count_str = v.get("$", "0")

                # 都道府県レベルや全国集計はスキップ
                if origin_code.endswith("000") or dest_code.endswith("000"):
                    continue
                if origin_code == "00000" or dest_code == "00000":
                    continue

                try:
                    count = int(count_str)
                except (ValueError, TypeError):
                    continue

                if count < MIN_COMMUTERS:
                    continue

                origin_pref, origin_muni = code_to_pref_muni(origin_code, area_map)
                dest_pref, dest_muni = code_to_pref_muni(dest_code, cat02_map)

                if not origin_muni or not dest_muni:
                    continue

                results.append({
                    "origin_pref": origin_pref,
                    "origin_muni": origin_muni,
                    "dest_pref": dest_pref,
                    "dest_muni": dest_muni,
                    "sex": sex_label,
                    "count": count,
                })

            # ページネーション
            total = d["GET_STATS_DATA"]["STATISTICAL_DATA"]["RESULT_INF"].get("TOTAL_NUMBER", 0)
            fetched = page * 100000
            if fetched >= int(total):
                break
            page += 1
            time.sleep(1)  # レート制限

        time.sleep(0.5)  # チャンク間待機

    return results


def insert_data(conn, all_data):
    """データをSQLiteに挿入（UPSERT方式）"""
    merged = {}
    for row in all_data:
        key = (row["origin_pref"], row["origin_muni"], row["dest_pref"], row["dest_muni"])
        if key not in merged:
            merged[key] = {"total": 0, "male": 0, "female": 0}
        merged[key][row["sex"]] = row["count"]

    conn.execute("DELETE FROM v2_external_commute_od")
    for (op, om, dp, dm), counts in merged.items():
        total = counts["total"] if counts["total"] > 0 else counts["male"] + counts["female"]
        if total < MIN_COMMUTERS:
            continue
        conn.execute(
            "INSERT OR REPLACE INTO v2_external_commute_od "
            "(origin_pref, origin_muni, dest_pref, dest_muni, total_commuters, male_commuters, female_commuters) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (op, om, dp, dm, total, counts["male"], counts["female"])
        )
    conn.commit()
    return len(merged)


def insert_data_incremental(conn, data):
    """1都道府県分のデータを逐次挿入+commit"""
    merged = {}
    for row in data:
        key = (row["origin_pref"], row["origin_muni"], row["dest_pref"], row["dest_muni"])
        if key not in merged:
            merged[key] = {"total": 0, "male": 0, "female": 0}
        merged[key][row["sex"]] = row["count"]

    count = 0
    for (op, om, dp, dm), counts in merged.items():
        total = counts["total"] if counts["total"] > 0 else counts["male"] + counts["female"]
        if total < MIN_COMMUTERS:
            continue
        conn.execute(
            "INSERT OR REPLACE INTO v2_external_commute_od "
            "(origin_pref, origin_muni, dest_pref, dest_muni, total_commuters, male_commuters, female_commuters) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (op, om, dp, dm, total, counts["male"], counts["female"])
        )
        count += 1
    conn.commit()
    return count


def compute_summaries(conn):
    """集約テーブルを計算"""
    conn.execute("DELETE FROM v2_commute_flow_summary")

    # Inflow: ある市区町村に通勤してくる人
    cursor = conn.execute("""
        SELECT dest_pref, dest_muni,
               SUM(total_commuters) as total,
               COUNT(*) as partner_count
        FROM v2_external_commute_od
        WHERE origin_pref != dest_pref OR origin_muni != dest_muni
        GROUP BY dest_pref, dest_muni
    """)
    for row in cursor.fetchall():
        pref, muni, total, partners = row
        # Self commute
        self_row = conn.execute(
            "SELECT total_commuters FROM v2_external_commute_od WHERE origin_pref=? AND origin_muni=? AND dest_pref=? AND dest_muni=?",
            (pref, muni, pref, muni)
        ).fetchone()
        self_count = self_row[0] if self_row else 0
        self_rate = self_count / (total + self_count) if (total + self_count) > 0 else 0

        # Top 10
        top10 = conn.execute("""
            SELECT origin_pref, origin_muni, total_commuters
            FROM v2_external_commute_od
            WHERE dest_pref=? AND dest_muni=? AND (origin_pref != dest_pref OR origin_muni != dest_muni)
            ORDER BY total_commuters DESC LIMIT 10
        """, (pref, muni)).fetchall()
        top10_json = json.dumps([{"pref": r[0], "muni": r[1], "count": r[2]} for r in top10], ensure_ascii=False)

        conn.execute(
            "INSERT OR REPLACE INTO v2_commute_flow_summary VALUES (?,?,?,?,?,?,?,?)",
            (pref, muni, "inflow", total, self_count, self_rate, partners, top10_json)
        )

    # Outflow: ある市区町村から出ていく人
    cursor = conn.execute("""
        SELECT origin_pref, origin_muni,
               SUM(total_commuters) as total,
               COUNT(*) as partner_count
        FROM v2_external_commute_od
        WHERE origin_pref != dest_pref OR origin_muni != dest_muni
        GROUP BY origin_pref, origin_muni
    """)
    for row in cursor.fetchall():
        pref, muni, total, partners = row
        self_row = conn.execute(
            "SELECT total_commuters FROM v2_external_commute_od WHERE origin_pref=? AND origin_muni=? AND dest_pref=? AND dest_muni=?",
            (pref, muni, pref, muni)
        ).fetchone()
        self_count = self_row[0] if self_row else 0
        self_rate = self_count / (total + self_count) if (total + self_count) > 0 else 0

        top10 = conn.execute("""
            SELECT dest_pref, dest_muni, total_commuters
            FROM v2_external_commute_od
            WHERE origin_pref=? AND origin_muni=? AND (origin_pref != dest_pref OR origin_muni != dest_muni)
            ORDER BY total_commuters DESC LIMIT 10
        """, (pref, muni)).fetchall()
        top10_json = json.dumps([{"pref": r[0], "muni": r[1], "count": r[2]} for r in top10], ensure_ascii=False)

        conn.execute(
            "INSERT OR REPLACE INTO v2_commute_flow_summary VALUES (?,?,?,?,?,?,?,?)",
            (pref, muni, "outflow", total, self_count, self_rate, partners, top10_json)
        )

    conn.commit()


def main():
    db_path = sys.argv[1] if len(sys.argv) > 1 else DB_PATH
    print(f"DB: {db_path}")

    conn = sqlite3.connect(db_path)
    create_tables(conn)

    print("Fetching area name mappings...")
    area_map, cat02_map = fetch_area_names(APP_ID)
    print(f"  Areas: {len(area_map)}, Destinations: {len(cat02_map)}")

    # 既に取得済みの県をスキップ
    existing_prefs = set()
    try:
        rows = conn.execute("SELECT DISTINCT origin_pref FROM v2_external_commute_od").fetchall()
        existing_prefs = {r[0] for r in rows}
        if existing_prefs:
            print(f"Already fetched: {len(existing_prefs)} prefectures, skipping")
    except:
        pass

    total_inserted = 0
    for pref_code in PREF_CODES:
        pref_name = PREF_NAMES[pref_code]
        if pref_name in existing_prefs:
            print(f"Skipping {pref_name} ({pref_code}/47) - already fetched")
            continue
        print(f"Fetching {pref_name} ({pref_code}/47)...", end=" ", flush=True)
        data = fetch_pref_data(APP_ID, pref_code, area_map, cat02_map)
        # 都道府県単位で逐次投入+commit（クラッシュ耐性）
        count = insert_data_incremental(conn, data)
        total_inserted += count
        print(f"{len(data)} rows -> {count} OD pairs")
        time.sleep(1)

    print(f"\nTotal inserted: {total_inserted} OD pairs")

    print("Computing flow summaries...")
    compute_summaries(conn)

    # 検証
    od_count = conn.execute("SELECT COUNT(*) FROM v2_external_commute_od").fetchone()[0]
    summary_count = conn.execute("SELECT COUNT(*) FROM v2_commute_flow_summary").fetchone()[0]
    print(f"\nResults: {od_count} OD pairs, {summary_count} summary rows")

    # サンプル表示
    print("\nSample: Tokyo Shinjuku inflow top 5:")
    for row in conn.execute("""
        SELECT origin_pref, origin_muni, total_commuters
        FROM v2_external_commute_od
        WHERE dest_pref='東京都' AND dest_muni='新宿区'
        ORDER BY total_commuters DESC LIMIT 5
    """):
        print(f"  {row[0]} {row[1]}: {row[2]:,} commuters")

    conn.close()
    print("\nDone!")


if __name__ == "__main__":
    main()
