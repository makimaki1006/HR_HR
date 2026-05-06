"""
municipality_recruiting_scores テーブル生成スクリプト

仕様:
- basis='resident' のみ生成 (~20,845 行: 1,895 muni × 11 occupation)
- 0-200 正規化 (100 = 全国平均) — METRICS.md の 0-100 スケールを 2倍
- METRICS.md の penalty_reduction (0-30%) 減衰式を適用
- salary_living_score は municipality_living_cost_proxy (Worker A) があれば使用、なければ NULL
- 人数列は一切作らない (指数 / ランク / 濃淡 のみ)

使用方法:
    python scripts/build_municipality_recruiting_scores.py --dry-run
    python scripts/build_municipality_recruiting_scores.py --apply
    python scripts/build_municipality_recruiting_scores.py --verify

参照: docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md
"""

from __future__ import annotations

import argparse
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "hellowork.db"
TABLE = "municipality_recruiting_scores"
SOURCE_NAME = "recruiting_scores_v1"
SOURCE_YEAR = 2020  # 国勢調査ベース (thickness と整合)

DDL = f"""
CREATE TABLE IF NOT EXISTS {TABLE} (
    municipality_code TEXT NOT NULL,
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis             TEXT NOT NULL CHECK (basis IN ('resident','workplace')),
    occupation_code   TEXT NOT NULL,
    occupation_name   TEXT NOT NULL,

    distribution_priority_score REAL NOT NULL,
    target_thickness_index      REAL,
    commute_access_score        REAL,
    competition_score           REAL,
    salary_living_score         REAL,

    rank_in_occupation INTEGER,
    rank_percentile    REAL,
    distribution_priority TEXT CHECK (distribution_priority IN ('S','A','B','C','D')),

    scenario_conservative_score INTEGER,
    scenario_standard_score     INTEGER,
    scenario_aggressive_score   INTEGER,

    data_label    TEXT NOT NULL CHECK (data_label IN ('estimated_beta','measured','derived')),
    source_name   TEXT NOT NULL,
    source_year   INTEGER NOT NULL,
    weight_source TEXT,
    estimate_grade TEXT,

    estimated_at TEXT NOT NULL DEFAULT (datetime('now')),

    PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
);
"""

INDEX_DDL = [
    f"CREATE INDEX IF NOT EXISTS idx_mrs_priority ON {TABLE} (occupation_code, distribution_priority_score DESC);",
    f"CREATE INDEX IF NOT EXISTS idx_mrs_rank     ON {TABLE} (occupation_code, rank_in_occupation);",
    f"CREATE INDEX IF NOT EXISTS idx_mrs_pref     ON {TABLE} (prefecture, occupation_code);",
    f"CREATE INDEX IF NOT EXISTS idx_mrs_basis    ON {TABLE} (basis, data_label);",
]


def normalize_to_200(values: dict[str, float]) -> dict[str, float]:
    """値辞書を 0-200 (平均 100) に正規化。pct_rank ベース。"""
    if not values:
        return {}
    sorted_keys = sorted(values.keys(), key=lambda k: values[k])
    n = len(sorted_keys)
    out = {}
    for i, k in enumerate(sorted_keys):
        # percentile 0..1 → 0..200
        pct = (i + 0.5) / n
        out[k] = round(pct * 200, 4)
    return out


def has_living_cost_table(conn: sqlite3.Connection) -> bool:
    row = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='municipality_living_cost_proxy'"
    ).fetchone()
    return row is not None


def fetch_living_cost(conn: sqlite3.Connection) -> dict[str, float]:
    """Worker A 由来。salary_real_terms_proxy or 同等列を読み 0-200 正規化。
    存在しない/列名不明なら空辞書。"""
    if not has_living_cost_table(conn):
        return {}
    # 列名を判定
    cols = [r[1] for r in conn.execute("PRAGMA table_info(municipality_living_cost_proxy)").fetchall()]
    cand = None
    for c in ("salary_real_terms_proxy", "real_wage_index", "salary_living_index", "salary_proxy"):
        if c in cols:
            cand = c
            break
    if cand is None:
        return {}
    raw = {}
    for r in conn.execute(f"SELECT municipality_code, {cand} FROM municipality_living_cost_proxy WHERE {cand} IS NOT NULL").fetchall():
        raw[r[0]] = float(r[1])
    return normalize_to_200(raw)


def fetch_commute_inflow_share(conn: sqlite3.Connection) -> dict[str, float]:
    """destination muni ごとの流入合計 share を集計し 0-200 正規化。"""
    raw = {}
    rows = conn.execute(
        """
        SELECT destination_municipality_code, SUM(flow_share)
        FROM commute_flow_summary
        WHERE occupation_group_code = 'all'
        GROUP BY destination_municipality_code
        """
    ).fetchall()
    for code, val in rows:
        if code and val is not None:
            raw[code] = float(val)
    return normalize_to_200(raw)


def fetch_competition_score(conn: sqlite3.Connection) -> dict[str, float]:
    """競合求人密度 = 同 muni の postings 件数 / resident occupation_population 合計。
    低いほど優位なので、簡略版として正規化値を 200 から引く (高いほど優位)。"""
    # postings table から市区町村別件数 (簡略: prefecture×municipality 名でなく code が無い場合は近似困難)
    # postings に市区町村コードがあるか確認、なければ pref+name 名寄せ
    cols = [r[1] for r in conn.execute("PRAGMA table_info(postings)").fetchall()]
    code_col = None
    for c in ("municipality_code", "muni_code", "city_code"):
        if c in cols:
            code_col = c
            break
    raw_density = {}
    if code_col:
        rows = conn.execute(f"SELECT {code_col}, COUNT(*) FROM postings WHERE {code_col} IS NOT NULL GROUP BY {code_col}").fetchall()
        post_count = {r[0]: int(r[1]) for r in rows if r[0]}
    else:
        # フォールバック: prefecture + work_municipality (もしあれば) の名寄せ
        post_count = {}
        if "prefecture" in cols and "municipality" in cols:
            rows = conn.execute("SELECT prefecture, municipality, COUNT(*) FROM postings WHERE prefecture IS NOT NULL AND municipality IS NOT NULL GROUP BY prefecture, municipality").fetchall()
            namekey_to_count = {(r[0], r[1]): int(r[2]) for r in rows if r[0] and r[1]}
            for code, pref, name in conn.execute("SELECT municipality_code, prefecture, municipality_name FROM municipality_code_master").fetchall():
                cnt = namekey_to_count.get((pref, name))
                if cnt is not None:
                    post_count[code] = cnt
    # 母集団 (workplace population sum - resident は estimate_index のみで実数なし)
    pop_rows = conn.execute(
        "SELECT municipality_code, SUM(population) FROM municipality_occupation_population WHERE basis='workplace' AND population IS NOT NULL GROUP BY municipality_code"
    ).fetchall()
    pop = {r[0]: int(r[1] or 0) for r in pop_rows}
    for code, p in pop.items():
        if p <= 0:
            continue
        density = post_count.get(code, 0) / p
        raw_density[code] = density
    if not raw_density:
        return {}
    norm = normalize_to_200(raw_density)
    # 競合密度が低い (=優位) ほど高スコアに反転
    return {k: round(200.0 - v, 4) for k, v in norm.items()}


def thickness_to_index_200(val: float) -> float:
    """thickness_index は 0..200 スケール (確認済 max=200)。そのまま使用。"""
    return float(val)


def compute_priority(percentile: float) -> str:
    if percentile <= 0.02:
        return "S"
    if percentile <= 0.05:
        return "A"
    if percentile <= 0.15:
        return "B"
    if percentile <= 0.50:
        return "C"
    return "D"


def build_rows(conn: sqlite3.Connection):
    print("[step 1/5] fetching auxiliary indices...")
    commute_idx = fetch_commute_inflow_share(conn)
    competition_idx = fetch_competition_score(conn)
    salary_idx = fetch_living_cost(conn)
    print(f"  commute_idx: {len(commute_idx)} muni")
    print(f"  competition_idx: {len(competition_idx)} muni")
    print(f"  salary_idx (living_cost_proxy): {len(salary_idx)} muni  {'(Worker A 未完了)' if not salary_idx else ''}")

    print("[step 2/5] fetching thickness rows...")
    thickness_rows = conn.execute(
        """
        SELECT municipality_code, prefecture, municipality_name, basis,
               occupation_code, occupation_name, thickness_index,
               scenario_conservative_index, scenario_standard_index, scenario_aggressive_index,
               weight_source, estimate_grade
        FROM v2_municipality_target_thickness
        WHERE basis='resident'
        """
    ).fetchall()
    print(f"  thickness rows: {len(thickness_rows)}")

    print("[step 3/5] computing scores per (muni, occ)...")
    # 各 occupation 内で raw_score を計算してから rank/percentile 算出
    # まず full row 計算
    interim = []  # (muni_code, occ_code, raw_score, ...)
    for r in thickness_rows:
        (muni, pref, mname, basis, occ_code, occ_name, thickness, scn_c, scn_s, scn_a, wsrc, egrade) = r
        target_idx = thickness_to_index_200(thickness)  # 0..200
        commute_v = commute_idx.get(muni)  # 0..200 or None
        compete_v = competition_idx.get(muni)  # 0..200 or None
        salary_v = salary_idx.get(muni)  # 0..200 or None

        # 加点要素 (METRICS.md 重み 30/20/25/25 を 4 要素に簡略化:
        # 本実装では adjacent_population_index は近接職種版が未準備のため
        # target_thickness 50% / commute_reach 25% / competition_score 15% / salary 10% に再配分)
        weights = {"target": 50.0, "commute": 25.0, "competition": 15.0, "salary": 10.0}
        positive_components = {"target": target_idx, "commute": commute_v, "competition": compete_v, "salary": salary_v}
        active_w = {k: w for k, w in weights.items() if positive_components[k] is not None}
        wsum = sum(active_w.values()) or 1.0
        positive_score = sum(positive_components[k] * (active_w[k] / wsum) for k in active_w)

        # penalty_reduction は競合密度の高さ (=competition_score の低さ=200-compete_v) を使う
        # 競合 score が低い (200-compete_v 高) ほどペナルティ高
        if compete_v is not None:
            penalty_raw = (200.0 - compete_v)  # 0..200
            penalty_reduction_pct = (penalty_raw / 200.0) * 30.0  # 0..30%
        else:
            penalty_reduction_pct = 0.0

        raw_score = positive_score * (1 - penalty_reduction_pct / 100.0)
        # clamp 0..200
        distribution_priority_score = max(0.0, min(200.0, raw_score))

        interim.append({
            "muni": muni, "pref": pref, "mname": mname, "basis": basis,
            "occ_code": occ_code, "occ_name": occ_name,
            "target_thickness_index": target_idx,
            "commute_access_score": commute_v,
            "competition_score": compete_v,
            "salary_living_score": salary_v,
            "distribution_priority_score": round(distribution_priority_score, 4),
            "scn_c": scn_c, "scn_s": scn_s, "scn_a": scn_a,
            "weight_source": wsrc, "estimate_grade": egrade,
        })

    print("[step 4/5] computing per-occupation rank/percentile/priority...")
    # occupation 別に rank
    by_occ: dict[str, list] = {}
    for row in interim:
        by_occ.setdefault(row["occ_code"], []).append(row)
    for occ, rows in by_occ.items():
        rows.sort(key=lambda x: x["distribution_priority_score"], reverse=True)
        n = len(rows)
        for i, row in enumerate(rows):
            rank = i + 1
            pct = rank / n
            row["rank_in_occupation"] = rank
            row["rank_percentile"] = round(pct, 4)
            row["distribution_priority"] = compute_priority(pct)

    # シナリオは thickness の scenario 列を採用 (1×/3×/5× 倍率)
    return interim


def insert_rows(conn: sqlite3.Connection, rows: list[dict], dry_run: bool = False) -> int:
    if dry_run:
        return len(rows)
    now = datetime.now(timezone.utc).isoformat()
    cur = conn.cursor()
    cur.execute(f"DELETE FROM {TABLE} WHERE basis='resident' AND source_year=?", (SOURCE_YEAR,))
    sql = f"""
    INSERT INTO {TABLE} (
        municipality_code, prefecture, municipality_name, basis,
        occupation_code, occupation_name,
        distribution_priority_score, target_thickness_index, commute_access_score,
        competition_score, salary_living_score,
        rank_in_occupation, rank_percentile, distribution_priority,
        scenario_conservative_score, scenario_standard_score, scenario_aggressive_score,
        data_label, source_name, source_year, weight_source, estimate_grade, estimated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """
    payload = [
        (
            r["muni"], r["pref"], r["mname"], r["basis"],
            r["occ_code"], r["occ_name"],
            r["distribution_priority_score"], r["target_thickness_index"], r["commute_access_score"],
            r["competition_score"], r["salary_living_score"],
            r["rank_in_occupation"], r["rank_percentile"], r["distribution_priority"],
            r["scn_c"], r["scn_s"], r["scn_a"],
            "estimated_beta", SOURCE_NAME, SOURCE_YEAR, r["weight_source"], r["estimate_grade"], now,
        )
        for r in rows
    ]
    cur.executemany(sql, payload)
    conn.commit()
    return cur.rowcount


def verify(conn: sqlite3.Connection) -> bool:
    print("\n=== VERIFY ===")
    checks = []

    # 1. 行数
    n = conn.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE basis='resident'").fetchone()[0]
    ok = 18000 <= n <= 22000
    checks.append((f"1. row count (resident)={n}", ok))

    # 2. PK 重複
    dup = conn.execute(
        f"SELECT COUNT(*) FROM (SELECT municipality_code, basis, occupation_code, source_year, COUNT(*) c FROM {TABLE} GROUP BY 1,2,3,4 HAVING c>1)"
    ).fetchone()[0]
    checks.append((f"2. PK dup count={dup}", dup == 0))

    # 3. occupation 11
    occ_n = conn.execute(f"SELECT COUNT(DISTINCT occupation_code) FROM {TABLE} WHERE basis='resident'").fetchone()[0]
    checks.append((f"3. distinct occupations={occ_n}", occ_n == 11))

    # 4. priority 分布
    dist = conn.execute(f"SELECT distribution_priority, COUNT(*) FROM {TABLE} WHERE basis='resident' GROUP BY distribution_priority ORDER BY distribution_priority").fetchall()
    has_all = set(d[0] for d in dist) >= {"S", "A", "B", "C", "D"}
    checks.append((f"4. priority dist={dist}", has_all))

    # 5. score range 0..200
    rng = conn.execute(f"SELECT MIN(distribution_priority_score), MAX(distribution_priority_score) FROM {TABLE} WHERE basis='resident'").fetchone()
    checks.append((f"5. score range={rng}", 0 <= rng[0] and rng[1] <= 200))

    # 6. NULL 率 (salary_living_score)
    null_n = conn.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE basis='resident' AND salary_living_score IS NULL").fetchone()[0]
    null_pct = (null_n / n) * 100 if n else 0
    checks.append((f"6. salary_living NULL%={null_pct:.1f}% ({null_n}/{n})", True))  # 許容 (Worker A 依存)

    # 7. master orphan
    orphan = conn.execute(f"""
        SELECT COUNT(*) FROM {TABLE} m WHERE basis='resident'
          AND NOT EXISTS (SELECT 1 FROM municipality_code_master x WHERE x.municipality_code = m.municipality_code)
    """).fetchone()[0]
    checks.append((f"7. master orphan={orphan}", orphan == 0))

    # 8. parent_rank 整合性 (designated_ward は parent と national rank を持つ — 本実装は national_rank のみ)
    dw_n = conn.execute(f"""
        SELECT COUNT(DISTINCT m.municipality_code) FROM {TABLE} m
          JOIN municipality_code_master x ON x.municipality_code = m.municipality_code
          WHERE x.is_designated_ward=1 AND m.basis='resident'
    """).fetchone()[0]
    checks.append((f"8. designated_ward muni in scores={dw_n} (expected ~175)", 150 <= dw_n <= 180))

    # 9. scenario 範囲
    scn_rng = conn.execute(f"SELECT MIN(scenario_conservative_score), MAX(scenario_aggressive_score) FROM {TABLE} WHERE basis='resident'").fetchone()
    checks.append((f"9. scenario range cons_min={scn_rng[0]}, agg_max={scn_rng[1]}", 0 <= (scn_rng[0] or 0) and (scn_rng[1] or 0) <= 1000))

    # 10. data_label
    lbl = conn.execute(f"SELECT data_label, COUNT(*) FROM {TABLE} WHERE basis='resident' GROUP BY data_label").fetchall()
    only_beta = len(lbl) == 1 and lbl[0][0] == "estimated_beta"
    checks.append((f"10. data_label dist={lbl}", only_beta))

    all_ok = True
    for desc, ok in checks:
        mark = "PASS" if ok else "FAIL"
        print(f"  [{mark}] {desc}")
        if not ok:
            all_ok = False
    return all_ok


def show_top10(conn: sqlite3.Connection) -> None:
    print("\n=== distribution_priority_score TOP 10 (across all occupations) ===")
    rows = conn.execute(
        f"SELECT prefecture, municipality_name, occupation_code, distribution_priority_score, rank_in_occupation, distribution_priority FROM {TABLE} WHERE basis='resident' ORDER BY distribution_priority_score DESC LIMIT 10"
    ).fetchall()
    for r in rows:
        print(f"  {r[0]} {r[1]} | {r[2]} | score={r[3]:.2f} rank={r[4]} priority={r[5]}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if not (args.dry_run or args.apply or args.verify):
        parser.print_help()
        return 1

    conn = sqlite3.connect(DB_PATH)
    try:
        conn.execute(DDL)
        for ix in INDEX_DDL:
            conn.execute(ix)
        conn.commit()

        if args.verify and not args.apply:
            ok = verify(conn)
            show_top10(conn)
            return 0 if ok else 2

        rows = build_rows(conn)
        print(f"[step 5/5] {'DRY-RUN' if args.dry_run else 'INSERT'} {len(rows)} rows")
        if args.dry_run:
            # サンプル 5 件表示
            for r in rows[:5]:
                print(f"  {r['muni']} {r['mname']} {r['occ_code']} score={r['distribution_priority_score']:.2f}")
            return 0
        n = insert_rows(conn, rows, dry_run=False)
        print(f"  inserted: {n}")
        ok = verify(conn)
        show_top10(conn)
        return 0 if ok else 2
    finally:
        conn.close()


if __name__ == "__main__":
    sys.exit(main())
