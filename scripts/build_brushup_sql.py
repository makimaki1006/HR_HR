"""
data/generated/brushup_qualifications.json から
Turso 投入用 SQL を生成し data/license_brushup_turso_import.sql に出力する。

テーブル: v2_external_license_brushup
PK: (jilpt_name, section_order)
"""
import json
import re
from datetime import datetime, timezone
from pathlib import Path


def escape_sql(s: str) -> str:
    """SQLite/Turso 向けにシングルクォートをエスケープする。"""
    return s.replace("'", "''")


def main() -> None:
    base_dir = Path(__file__).resolve().parent.parent
    json_path = base_dir / 'data' / 'generated' / 'brushup_qualifications.json'
    sql_path = base_dir / 'data' / 'license_brushup_turso_import.sql'

    if not json_path.exists():
        raise FileNotFoundError(f'JSON が見つかりません: {json_path}')

    with open(json_path, encoding='utf-8') as f:
        data = json.load(f)

    fetched_at = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

    lines: list[str] = []

    # DDL
    lines.append('-- ============================================================')
    lines.append('-- v2_external_license_brushup  Turso 投入 SQL')
    lines.append(f'-- 生成日時: {fetched_at}')
    lines.append(f'-- エントリ数 (JSON): {len(data)}')
    lines.append('-- ============================================================')
    lines.append('')
    lines.append('CREATE TABLE IF NOT EXISTS v2_external_license_brushup (')
    lines.append('    jilpt_name    TEXT    NOT NULL,')
    lines.append('    brushup_url   TEXT    NOT NULL,')
    lines.append('    brushup_name  TEXT    NOT NULL,')
    lines.append('    section_order INTEGER NOT NULL,')
    lines.append('    section_h2    TEXT    NOT NULL,')
    lines.append('    section_body  TEXT    NOT NULL,')
    lines.append("    fetched_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,")
    lines.append('    PRIMARY KEY (jilpt_name, section_order)')
    lines.append(');')
    lines.append('CREATE INDEX IF NOT EXISTS idx_license_brushup_name')
    lines.append('    ON v2_external_license_brushup(jilpt_name);')
    lines.append('')

    # DELETE (冪等性確保)
    lines.append('-- 既存データ削除 (冪等実行対応)')
    lines.append('DELETE FROM v2_external_license_brushup;')
    lines.append('')

    # INSERT
    insert_count = 0
    skipped_no_sections = 0

    for entry in data:
        jilpt_name = entry.get('jilpt_name', '')
        brushup_url = entry.get('brushup_url', '')
        brushup_name = entry.get('brushup_name', '')
        sections = entry.get('sections', [])

        if not sections:
            skipped_no_sections += 1
            continue

        for order, sec in enumerate(sections):
            h2 = sec.get('h2', '')
            body = sec.get('body', '')
            if not h2 and not body:
                continue

            j_esc = escape_sql(jilpt_name)
            u_esc = escape_sql(brushup_url)
            b_esc = escape_sql(brushup_name)
            h_esc = escape_sql(h2)
            body_esc = escape_sql(body)

            lines.append(
                f"INSERT INTO v2_external_license_brushup "
                f"(jilpt_name, brushup_url, brushup_name, section_order, section_h2, section_body, fetched_at) "
                f"VALUES ('{j_esc}', '{u_esc}', '{b_esc}', {order}, '{h_esc}', '{body_esc}', '{fetched_at}');"
            )
            insert_count += 1

    # フッター統計
    lines.append('')
    lines.append(f'-- INSERT 件数: {insert_count}')
    lines.append(f'-- セクションなしスキップ: {skipped_no_sections}')

    sql_path.write_text('\n'.join(lines), encoding='utf-8')

    print(f'[build_brushup_sql] SQL 出力: {sql_path}')
    print(f'  INSERT 行数: {insert_count}')
    print(f'  スキップ (セクションなし): {skipped_no_sections}')

    # サマリー表示
    print('\n--- サンプル (最初の3エントリ) ---')
    shown = 0
    for entry in data:
        if not entry.get('sections'):
            continue
        print(f"  jilpt_name : {entry['jilpt_name']}")
        print(f"  brushup_url: {entry['brushup_url']}")
        h2_list = [s['h2'] for s in entry['sections']]
        print(f"  h2 一覧    : {h2_list}")
        print()
        shown += 1
        if shown >= 3:
            break


if __name__ == '__main__':
    main()
