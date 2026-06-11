"""data/jobtag_turso_import.sql を Turso country-statistics に投入する。

実装方針:
  - 既存 upload_to_turso.py 系と同じく requests / libsql_client 経由
  - turso CLI に依存しない（PowerShell環境で `<` リダイレクト不可のため）
  - --dry-run で接続のみ確認、--apply で実際に書き込み
  - SQLファイルを ; で分割し、CREATE/INSERT を順次 execute

環境変数:
  TURSO_EXTERNAL_URL   Turso DB URL (libsql:// or https://)
  TURSO_EXTERNAL_TOKEN Turso Auth Token
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

from libsql_client import create_client_sync

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SQL = ROOT / "data" / "jobtag_turso_import.sql"
DEFAULT_ENV = ROOT / ".env"


def load_dotenv(path: Path) -> int:
    """KEY=VALUE 形式の .env を読み込み os.environ に未設定キーだけ反映する。

    既に環境変数が設定されている場合は上書きしない（CI環境等を尊重）。
    クォート("/')で囲まれた値はクォートを剥がす。
    """
    if not path.exists():
        return 0
    loaded = 0
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        # export KEY=... 形式も対応
        if line.startswith("export "):
            line = line[len("export "):]
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if value and value[0] == value[-1] and value[0] in {'"', "'"}:
            value = value[1:-1]
        if key and key not in os.environ:
            os.environ[key] = value
            loaded += 1
    return loaded


def split_sql(text: str) -> list[str]:
    """SQLファイルを文に分割。

    - 行頭の `--` コメント行は除外
    - 文字列リテラル内の `;` は無視（シングルクォート、SQLite 規約 `''` エスケープ対応）
    """
    out: list[str] = []
    buf: list[str] = []
    in_str = False
    i = 0
    # まずコメント行を除去
    lines = []
    for line in text.split("\n"):
        stripped = line.lstrip()
        if stripped.startswith("--"):
            continue
        lines.append(line)
    src = "\n".join(lines)

    while i < len(src):
        ch = src[i]
        if ch == "'":
            # SQLite の '' エスケープを考慮: 連続する '' は文字列終端ではない
            if in_str and i + 1 < len(src) and src[i + 1] == "'":
                buf.append("''")
                i += 2
                continue
            in_str = not in_str
            buf.append(ch)
        elif ch == ";" and not in_str:
            stmt = "".join(buf).strip()
            if stmt:
                out.append(stmt)
            buf = []
        else:
            buf.append(ch)
        i += 1
    # 末尾の余り
    tail = "".join(buf).strip()
    if tail:
        out.append(tail)
    return out


def classify(stmt: str) -> str:
    """分類用ラベル（CREATE/INSERT/DELETE/BEGIN/COMMIT/その他）。"""
    head = stmt.lstrip().split(None, 1)[0].upper() if stmt.strip() else ""
    if head in {"CREATE", "INSERT", "DELETE", "BEGIN", "COMMIT", "DROP"}:
        return head
    return "OTHER"


def main() -> int:
    parser = argparse.ArgumentParser(description="jobtag_driver SQL を Turso に投入する")
    parser.add_argument("--sql", type=Path, default=DEFAULT_SQL, help=f"SQLファイル（既定: {DEFAULT_SQL.name}）")
    parser.add_argument("--dry-run", action="store_true", help="接続と分割確認のみ（書き込みなし）")
    parser.add_argument("--apply", action="store_true", help="実際に Turso へ書き込む（明示的に必要）")
    parser.add_argument("--url", default=None, help="Turso URL（既定: $TURSO_EXTERNAL_URL）")
    parser.add_argument("--token", default=None, help="Turso Token（既定: $TURSO_EXTERNAL_TOKEN）")
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV,
                        help=f"環境変数ファイル（既定: {DEFAULT_ENV.name}、未設定キーのみ取り込み）")
    args = parser.parse_args()

    sys.stdout.reconfigure(encoding="utf-8")

    # .env 自動読み込み（既存環境変数は尊重）
    loaded = load_dotenv(args.env_file)
    if loaded:
        print(f".env から {loaded} 件の変数を読み込みました（既存環境変数は保持）")

    if not args.sql.exists():
        print(f"ERROR: SQLファイルが存在しません: {args.sql}", file=sys.stderr)
        return 1

    text = args.sql.read_text(encoding="utf-8")
    statements = split_sql(text)
    counts: dict[str, int] = {}
    for s in statements:
        c = classify(s)
        counts[c] = counts.get(c, 0) + 1

    print(f"SQL: {args.sql}")
    print(f"文の総数: {len(statements)}")
    for k in sorted(counts.keys()):
        print(f"  {k:>8s}: {counts[k]}")

    if not args.apply and not args.dry_run:
        print()
        print("実行モード未指定。次のいずれかで再実行してください:")
        print("  --dry-run  ... 接続テストのみ（書き込みなし）")
        print("  --apply    ... 実際に Turso に書き込む（規約上ユーザー手動承認）")
        return 0

    url = args.url or os.environ.get("TURSO_EXTERNAL_URL", "")
    token = args.token or os.environ.get("TURSO_EXTERNAL_TOKEN", "")
    if not url or not token:
        print("ERROR: TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN を設定してください", file=sys.stderr)
        print("       例: $env:TURSO_EXTERNAL_URL = 'libsql://...'", file=sys.stderr)
        return 2

    # libsql:// → https:// 変換（既存 turso_http.rs と同じ）
    if url.startswith("libsql://"):
        url = "https://" + url[len("libsql://"):]

    print()
    print(f"接続先: {url}")
    print(f"モード: {'apply (実書き込み)' if args.apply else 'dry-run (接続テストのみ)'}")
    print()

    try:
        with create_client_sync(url, auth_token=token) as client:
            # 接続テスト
            r = client.execute("SELECT 1 AS ok")
            ok_val = r.rows[0][0] if r.rows else None
            print(f"接続テスト: SELECT 1 → {ok_val}")

            if not args.apply:
                # dry-run: 既存テーブル数だけ確認
                try:
                    r = client.execute(
                        "SELECT COUNT(*) FROM sqlite_master "
                        "WHERE type='table' AND name LIKE 'v2_external_jobtag_%'"
                    )
                    existing = r.rows[0][0] if r.rows else 0
                    print(f"既存 v2_external_jobtag_* テーブル数: {existing} (期待: 投入前なら 0)")
                except Exception as e:
                    print(f"テーブル数取得スキップ: {e}")
                print()
                print("dry-run 完了。--apply で実書き込みを実行できます。")
                return 0

            # 本番投入
            print("=== 投入開始 ===")
            # BEGIN/COMMIT は libsql_client では明示的に扱わず、batch で投入
            # （batch は自動でトランザクションになる）
            exec_targets = [s for s in statements if classify(s) not in {"BEGIN", "COMMIT"}]
            print(f"投入対象: {len(exec_targets)} 文（BEGIN/COMMIT を除く）")

            applied = 0
            errors: list[tuple[int, str, str]] = []
            for idx, stmt in enumerate(exec_targets, start=1):
                try:
                    client.execute(stmt)
                    applied += 1
                    if idx % 100 == 0:
                        print(f"  {idx}/{len(exec_targets)} 完了")
                except Exception as e:
                    errors.append((idx, stmt[:80], str(e)))
                    if len(errors) >= 5:
                        print(f"エラーが {len(errors)} 件発生したため中断します")
                        break

            print()
            print(f"=== 結果 ===")
            print(f"投入成功: {applied}/{len(exec_targets)}")
            if errors:
                print(f"エラー: {len(errors)} 件")
                for idx, head, err in errors:
                    print(f"  [{idx}] {head}... → {err[:120]}")
                return 3
            print("全文の投入が成功しました")

            # 検証クエリ
            print()
            print("=== 検証クエリ ===")
            for sql, label in [
                ("SELECT category, COUNT(*) FROM v2_external_jobtag_occupation GROUP BY category", "category別件数"),
                ("SELECT wage_census_code, COUNT(*) FROM v2_external_jobtag_wage_age GROUP BY wage_census_code", "wage_code別件数"),
                ("SELECT COUNT(*) FROM v2_external_jobtag_description", "解説件数"),
                ("SELECT COUNT(*) FROM v2_external_jobtag_scores", "スコア件数"),
                ("SELECT COUNT(*) FROM v2_external_jobtag_qualifications", "資格件数"),
            ]:
                try:
                    r = client.execute(sql)
                    print(f"{label}:")
                    for row in r.rows:
                        print(f"  {list(row)}")
                except Exception as e:
                    print(f"{label}: ERROR {e}")
            return 0

    except Exception as e:
        print(f"FATAL: {e}", file=sys.stderr)
        return 4


if __name__ == "__main__":
    sys.exit(main())
