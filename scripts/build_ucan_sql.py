"""
data/generated/ucan_qualifications.json から
Turso 投入用 SQL を生成し data/license_ucan_turso_import.sql に出力する。

テーブル: v2_external_license_ucan
PK: (jilpt_name, section_order)
"""

import json
from datetime import datetime, timezone
from pathlib import Path


def escape_sql(s: str) -> str:
    """SQLite/Turso 向けにシングルクォートをエスケープする。"""
    return s.replace("'", "''")


def main() -> None:
    import sys
    sys.stdout.reconfigure(encoding='utf-8')

    base_dir = Path(__file__).resolve().parent.parent
    json_path = base_dir / 'data' / 'generated' / 'ucan_qualifications.json'
    sql_path = base_dir / 'data' / 'license_ucan_turso_import.sql'

    if not json_path.exists():
        raise FileNotFoundError(
            f'JSON が見つかりません: {json_path}\n'
            f'先に scripts/fetch_ucan.py を実行してください。'
        )

    with open(json_path, encoding='utf-8') as f:
        data: list[dict] = json.load(f)

    fetched_at = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

    lines: list[str] = []

    # ヘッダー
    lines.append('-- ============================================================')
    lines.append('-- v2_external_license_ucan  Turso 投入 SQL')
    lines.append(f'-- 生成日時: {fetched_at}')
    lines.append(f'-- エントリ数 (JSON): {len(data)}')
    lines.append('-- ============================================================')
    lines.append('')

    # DDL
    lines.append('CREATE TABLE IF NOT EXISTS v2_external_license_ucan (')
    lines.append('    jilpt_name    TEXT    NOT NULL,')
    lines.append('    ucan_url      TEXT    NOT NULL,')
    lines.append('    ucan_title    TEXT    NOT NULL,')
    lines.append('    section_order INTEGER NOT NULL,')
    lines.append('    section_h2    TEXT    NOT NULL,')
    lines.append('    section_body  TEXT    NOT NULL,')
    lines.append("    fetched_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,")
    lines.append('    PRIMARY KEY (jilpt_name, section_order)')
    lines.append(');')
    lines.append('CREATE INDEX IF NOT EXISTS idx_license_ucan_name')
    lines.append('    ON v2_external_license_ucan(jilpt_name);')
    lines.append('')

    # DELETE (冪等性確保)
    lines.append('-- 既存データ削除 (冪等実行対応)')
    lines.append('DELETE FROM v2_external_license_ucan;')
    lines.append('')

    # INSERT
    insert_count = 0
    skipped_no_sections = 0

    for entry in data:
        jilpt_name = entry.get('jilpt_name', '')
        ucan_url = entry.get('ucan_url', '')
        ucan_title = entry.get('ucan_title', '')
        entry_fetched_at = entry.get('fetched_at', fetched_at)
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
            u_esc = escape_sql(ucan_url)
            t_esc = escape_sql(ucan_title)
            h_esc = escape_sql(h2)
            body_esc = escape_sql(body)
            fa_esc = escape_sql(entry_fetched_at)

            lines.append(
                f"INSERT INTO v2_external_license_ucan "
                f"(jilpt_name, ucan_url, ucan_title, section_order, section_h2, section_body, fetched_at) "
                f"VALUES ('{j_esc}', '{u_esc}', '{t_esc}', {order}, '{h_esc}', '{body_esc}', '{fa_esc}');"
            )
            insert_count += 1

    # フッター統計
    lines.append('')
    lines.append(f'-- INSERT 件数: {insert_count}')
    lines.append(f'-- セクションなしスキップ: {skipped_no_sections}')

    sql_path.write_text('\n'.join(lines), encoding='utf-8')

    print(f'[build_ucan_sql] SQL 出力: {sql_path}')
    print(f'  INSERT 行数: {insert_count}')
    print(f'  スキップ (セクションなし): {skipped_no_sections}')

    # サマリー表示
    print('\n--- サンプル (最初の3エントリ) ---')
    shown = 0
    for entry in data:
        if not entry.get('sections'):
            continue
        print(f"  jilpt_name : {entry['jilpt_name']}")
        print(f"  ucan_url   : {entry['ucan_url']}")
        print(f"  ucan_title : {entry['ucan_title']}")
        h2_list = [s['h2'] for s in entry['sections']]
        print(f"  h2 一覧    : {h2_list}")
        print()
        shown += 1
        if shown >= 3:
            break


if __name__ == '__main__':
    main()
