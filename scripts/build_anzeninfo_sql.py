"""anzeninfo_qualifications.json → Turso 投入用 SQL を生成"""
import json
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

JSON_IN = Path("data/generated/anzeninfo_qualifications.json")
SQL_OUT = Path("data/license_anzeninfo_turso_import.sql")

DDL = """
CREATE TABLE IF NOT EXISTS v2_external_license_anzeninfo (
    jilpt_name TEXT NOT NULL,
    anzeninfo_url TEXT NOT NULL,
    anzeninfo_title TEXT NOT NULL,
    section_order INTEGER NOT NULL,
    section_h2 TEXT NOT NULL,
    section_body TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (jilpt_name, section_order)
);
CREATE INDEX IF NOT EXISTS idx_license_anzeninfo_name ON v2_external_license_anzeninfo(jilpt_name);
""".strip()


def esc(s: str) -> str:
    return s.replace("'", "''")


def main() -> int:
    data = json.loads(JSON_IN.read_text(encoding="utf-8"))
    lines = ["BEGIN;", DDL]
    inserted = 0
    for entry in data:
        jname = esc(entry["jilpt_name"])
        url = esc(entry.get("anzeninfo_url", ""))
        title = esc(entry.get("anzeninfo_title", ""))
        lines.append(f"DELETE FROM v2_external_license_anzeninfo WHERE jilpt_name = '{jname}';")
        for idx, sec in enumerate(entry.get("sections", [])):
            h2 = esc(sec.get("h2", ""))
            body = esc(sec.get("body", ""))
            lines.append(
                "INSERT INTO v2_external_license_anzeninfo "
                "(jilpt_name, anzeninfo_url, anzeninfo_title, section_order, section_h2, section_body) "
                f"VALUES ('{jname}', '{url}', '{title}', {idx}, '{h2}', '{body}');"
            )
            inserted += 1
    lines.append("COMMIT;")
    SQL_OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"SQL written: {SQL_OUT} ({SQL_OUT.stat().st_size:,} bytes, {inserted} INSERT)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
