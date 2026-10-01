"""Generate a reviewable, limited transaction. This CLI never writes to a DB.

python scripts/update_minimum_wages.py --as-of 2026-10-01 --output target/minimum-wage-update.sql
Optional --schema-db inspects an existing SQLite DB read-only before generating SQL.
Execute the reviewed transaction once using a SQL client that stops on errors and
rolls back. Turso writes are user-only under CLAUDE.md.
"""
import argparse
import sqlite3
import json
import os
import urllib.request
from pathlib import Path
from minimum_wage_rates import DEFAULT_SOURCE, NATIONAL_AVERAGES, load_rates, select_current, today_jst

CURRENT = "v2_external_minimum_wage"
HISTORY = "v2_external_minimum_wage_history"


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def inspect_schema(path):
    """Read-only schema gate: both tables and their actual conflict keys must exist."""
    uri = Path(path).resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        schemas = {}
        for table, required, key in (
            (CURRENT, {"prefecture", "hourly_min_wage", "effective_date", "fiscal_year"}, ("prefecture",)),
            (HISTORY, {"prefecture", "hourly_min_wage", "fiscal_year"}, ("fiscal_year", "prefecture")),
        ):
            info = db.execute(f"PRAGMA table_info({table})").fetchall()
            columns = {r[1] for r in info}
            pk = tuple(r[1] for r in sorted(info, key=lambda r: r[5]) if r[5])
            unique = [pk]
            for index in db.execute(f"PRAGMA index_list({table})").fetchall():
                if index[2]:
                    unique.append(tuple(r[2] for r in db.execute(f"PRAGMA index_info({quote(index[1])})")))
            if not required.issubset(columns) or not any(set(k) == set(key) for k in unique):
                raise ValueError(f"Incompatible schema/key for {table}; no SQL generated")
            schemas[table] = columns
        derived = [r[0] for r in db.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name IN "
            "('v2_wage_compliance','v2_region_benchmark','cross_wage_public','municipality_living_cost_proxy')")]
    return schemas, derived


def inspect_turso():
    """Inspect the deployed schema with SELECT/PRAGMA only. Never log credentials."""
    url = os.environ.get("TURSO_EXTERNAL_URL", "").replace("libsql://", "https://").rstrip("/")
    token = os.environ.get("TURSO_EXTERNAL_TOKEN", "")
    if not url.startswith("https://") or not token:
        raise ValueError("Set TURSO_EXTERNAL_URL and TURSO_EXTERNAL_TOKEN for read-only schema inspection")

    def query(sql):
        payload = {"requests": [{"type": "execute", "stmt": {"sql": sql}}, {"type": "close"}]}
        request = urllib.request.Request(url + "/v2/pipeline", data=json.dumps(payload).encode(),
                                        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.load(response)["results"][0]
        except Exception:
            raise ValueError("Read-only schema request failed; check connection/credentials") from None
        if result.get("type") != "ok":
            raise ValueError("Read-only schema query failed")
        return [[cell.get("value") for cell in row] for row in result["response"]["result"]["rows"]]

    schemas = {}
    for table, required, key in (
        (CURRENT, {"prefecture", "hourly_min_wage", "effective_date", "fiscal_year"}, {"prefecture"}),
        (HISTORY, {"prefecture", "hourly_min_wage", "fiscal_year"}, {"fiscal_year", "prefecture"}),
    ):
        info = query(f"PRAGMA table_info({table})")
        columns = {r[1] for r in info}
        unique = [{r[1] for r in info if int(r[5] or 0)}]
        for index in query(f"PRAGMA index_list({table})"):
            if int(index[2] or 0):
                unique.append({r[2] for r in query(f"PRAGMA index_info({quote(index[1])})")})
        if not required.issubset(columns) or key not in unique:
            raise ValueError(f"Incompatible schema/key for {table}; no SQL generated")
        schemas[table] = columns
    derived = [r[0] for r in query("SELECT name FROM sqlite_master WHERE type='table' AND name IN "
                                  "('v2_wage_compliance','v2_region_benchmark','cross_wage_public','municipality_living_cost_proxy')")]
    return schemas, derived


def upsert(table, columns, rows, key):
    updates = ", ".join(f"{c}=excluded.{c}" for c in columns if c not in key)
    values = ",\n".join("(" + ",".join("NULL" if v is None else str(v) if isinstance(v, (int, float)) else quote(v) for v in row) + ")" for row in rows)
    return f"INSERT INTO {table} ({','.join(columns)}) VALUES\n{values}\nON CONFLICT ({','.join(key)}) DO UPDATE SET {updates};\n"


def build_sql(rates, as_of=None, schemas=None, history_only=False, legacy_history=()):
    as_of = as_of or today_jst().isoformat()
    current = select_current(rates, as_of)
    lines = [f"-- As-of (Japan): {as_of}\n", "-- Generated only; review schema and execute once with stop-on-error / rollback.\n",
             "-- Current uses effective dates; history contains published annual rates, including future dates.\n",
             "-- Annual national averages are final revised averages, not a mixed-date national average.\n",
             "BEGIN IMMEDIATE;\n"]
    if not history_only:
        rows = [(r.prefecture, r.hourly_min_wage, r.effective_date.isoformat(), r.fiscal_year)
                for r in sorted(current.values(), key=lambda r: r.prefecture)]
        lines.append(upsert(CURRENT, ["prefecture", "hourly_min_wage", "effective_date", "fiscal_year"], rows, ["prefecture"]))
    history = {(y, p): (y, p, w) for y, p, w in legacy_history}
    history.update({(r.fiscal_year, r.prefecture): (r.fiscal_year, r.prefecture, r.hourly_min_wage) for r in rates})
    for year in {r.fiscal_year for r in rates}:
        if year in NATIONAL_AVERAGES:
            history[year, "全国"] = year, "全国", NATIONAL_AVERAGES[year]
    columns = ["fiscal_year", "prefecture", "hourly_min_wage"]
    rows = [history[k] for k in sorted(history)]
    if schemas and "effective_date" in schemas[HISTORY]:
        dates = {(r.fiscal_year, r.prefecture): r.effective_date.isoformat() for r in rates}
        # Optional metadata must not be invented for old national/prefecture rows.
        sourced = [row for row in rows if (row[0], row[1]) in dates]
        other = [row for row in rows if (row[0], row[1]) not in dates]
        if other:
            lines.append(upsert(HISTORY, columns, other, ["fiscal_year", "prefecture"]))
        lines.append(upsert(HISTORY, columns + ["effective_date"],
                            [row + (dates[row[0], row[1]],) for row in sourced], ["fiscal_year", "prefecture"]))
    else:
        lines.append(upsert(HISTORY, columns, rows, ["fiscal_year", "prefecture"]))
    lines.append("COMMIT;\n")
    lines.append("-- Read-only verification after commit (check all 47 values against the source):\n")
    lines.append(f"SELECT prefecture,hourly_min_wage,effective_date,fiscal_year FROM {CURRENT} ORDER BY prefecture;\n")
    lines.append(f"SELECT fiscal_year,COUNT(*) AS rows,MIN(hourly_min_wage),MAX(hourly_min_wage) FROM {HISTORY} GROUP BY fiscal_year;\n")
    lines.append("-- Recompute dependent tables separately before publishing their comparisons:\n")
    lines.append("-- python scripts/revalue_minimum_wage_dependencies.py --as-of YYYY-MM-DD --tables compliance benchmark --output target/wage-derived.sql\n")
    lines.append("-- cross_wage_public: same CLI with --tables cross; month-end effective-date basis; historical rows outside CSV coverage retained.\n")
    lines.append("-- municipality_living_cost_proxy: build_municipality_living_cost_proxy.py --dry-run --as-of YYYY-MM-DD --output target/wage-cost.sql\n")
    return "".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--as-of", help="ISO effective-date cutoff (default today)")
    parser.add_argument("--schema-db", type=Path, help="Existing SQLite DB to inspect read-only")
    parser.add_argument("--turso-inspect", action="store_true", help="Read-only deployed schema check using TURSO_EXTERNAL_URL/TOKEN")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.schema_db and args.turso_inspect:
        parser.error("Choose either --schema-db or --turso-inspect")
    schemas, derived = inspect_schema(args.schema_db) if args.schema_db else inspect_turso() if args.turso_inspect else (None, [])
    sql = build_sql(load_rates(args.source), args.as_of, schemas)
    args.output.write_text(sql, encoding="utf-8")
    print(f"Generated {args.output}; no database writes.")
    print("Schema inspected." if schemas else "Schema NOT inspected; confirm required tables and keys before user execution.")
    if derived:
        print("Dependent tables found; their old values require separate recomputation: " + ", ".join(derived))


if __name__ == "__main__":
    main()
