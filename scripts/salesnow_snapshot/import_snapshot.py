# -*- coding: utf-8 -*-
"""スナップショット SQL を Turso へ投入する (turso CLI 不要)。

# なぜ CLI を使わないか

`turso db shell <db> < file.sql` は
  - PowerShell では `<` が予約語で使えない (`RedirectionNotSupported`)
  - そもそも turso CLI の導入が必要
という 2 つの前提を要求する。

アプリ本体 (`src/db/turso_http.rs`) は libSQL の HTTP Pipeline API を直接叩いており、
同じ経路を使えば CLI も `<` も要らない。

# 使い方 (PowerShell / bash 共通)

    # 認証情報 (アプリと同じ環境変数)
    $env:SALESNOW_TURSO_URL   = "https://<db>-<org>.turso.io"
    $env:SALESNOW_TURSO_TOKEN = "<token>"

    # 何が起きるかだけ見る (書き込まない)
    python scripts/salesnow_snapshot/import_snapshot.py --dry-run

    # 投入
    python scripts/salesnow_snapshot/import_snapshot.py

# 安全側の設計

- **事前確認**: 同じ snapshot_date が既に入っていれば、書き込まずに終了する。
  誤って 2 回流して枠を消費する事故を防ぐ (`feedback_turso_upload_once`)。
- **DDL 先行**: CREATE TABLE / CREATE INDEX を先に流し、失敗したらそこで止める。
- **バッチ単位で進捗表示**: 途中で落ちてもどこまで入ったか分かる。
  SQL は `INSERT OR IGNORE` なので、再実行しても既存行には課金されない。
- **事後検証**: 行数と従業員数合計を読み出して、期待値と突き合わせる。
"""
import argparse
import glob
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_OUTDIR = os.path.join(SCRIPT_DIR, "out")
TABLE = "v2_salesnow_headcount_snapshot"


def die(msg):
    sys.exit(f"[中止] {msg}")


def load_dotenv(path=".env"):
    """`.env` を読んで環境変数に載せる (既存の環境変数は上書きしない)。

    アプリ本体は `main.rs:13` で `dotenvy::dotenv()` を呼んでおり、
    リポジトリ直下の `.env` から認証情報を読む。投入スクリプトだけ
    環境変数を要求すると、同じ情報を 2 箇所に置くことになるため揃える。

    dotenvy と同じく、既に設定されている環境変数を上書きしない。
    戻り値は読み込んだキー名の集合 (値は返さない/出力しない)。
    """
    loaded = set()
    if not os.path.isfile(path):
        return loaded
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip().strip('"').strip("'")
            if key and key not in os.environ:
                os.environ[key] = val
                loaded.add(key)
    return loaded


def resolve_sql(arg):
    if arg:
        if not os.path.isfile(arg):
            die(f"指定されたファイルがない: {arg}")
        return arg
    found = sorted(glob.glob(os.path.join(DEFAULT_OUTDIR, "snapshot_*.sql")))
    if not found:
        die(
            f"投入する SQL が無い: {DEFAULT_OUTDIR}/snapshot_*.sql\n"
            "  先に生成すること:\n"
            "    python scripts/salesnow_snapshot/build_snapshot.py "
            '--source "<csv>" --date YYYY-MM-DD'
        )
    return found[-1]


def normalize_url(u):
    """libsql:// でも https:// でも受ける。"""
    u = u.strip().rstrip("/")
    if u.startswith("libsql://"):
        u = "https://" + u[len("libsql://"):]
    if not u.startswith("http"):
        u = "https://" + u
    return u


class Turso:
    """src/db/turso_http.rs と同じ /v2/pipeline を叩く最小クライアント。"""

    def __init__(self, url, token, timeout=120):
        self.url = normalize_url(url) + "/v2/pipeline"
        self.token = token
        self.timeout = timeout

    def execute(self, sql):
        payload = {
            "requests": [
                {"type": "execute", "stmt": {"sql": sql}},
                {"type": "close"},
            ]
        }
        req = urllib.request.Request(
            self.url,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as r:
                data = json.loads(r.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            die(f"HTTP {e.code}: {e.read().decode('utf-8', 'replace')[:300]}")
        except Exception as e:
            die(f"接続失敗: {e}")

        for res in data.get("results", []):
            if res.get("type") == "error":
                die("SQL エラー: " + str(res.get("error", {}).get("message")))
        first = (data.get("results") or [{}])[0]
        return (first.get("response") or {}).get("result")

    def scalar(self, sql):
        r = self.execute(sql)
        rows = (r or {}).get("rows") or []
        if not rows or not rows[0]:
            return None
        cell = rows[0][0]
        if cell.get("type") == "null":
            return None
        v = cell.get("value")
        return int(v) if cell.get("type") == "integer" else v


def split_statements(sql_text):
    """DDL と INSERT バッチに分ける。`;` で終わる文単位。"""
    body = "\n".join(
        line for line in sql_text.splitlines() if not line.lstrip().startswith("--")
    )
    stmts = [s.strip() for s in body.split(";") if s.strip()]
    ddl = [s for s in stmts if s.upper().startswith("CREATE")]
    ins = [s for s in stmts if s.upper().startswith("INSERT")]
    other = [s for s in stmts if s not in ddl and s not in ins]
    if other:
        die(f"想定外の文が含まれている ({len(other)} 件): {other[0][:120]}")
    return ddl, ins


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("sql", nargs="?", help="投入する snapshot_*.sql (省略時 out/ の最新)")
    ap.add_argument("--dry-run", action="store_true", help="書き込まず、計画だけ表示")
    ap.add_argument("--url-env", default="SALESNOW_TURSO_URL")
    ap.add_argument("--token-env", default="SALESNOW_TURSO_TOKEN")
    ap.add_argument("--env-file", default=".env", help="認証情報を読む .env (既定: リポジトリ直下)")
    args = ap.parse_args()

    path = resolve_sql(args.sql)
    text = open(path, encoding="utf-8").read()
    ddl, ins = split_statements(text)
    rows = len(re.findall(r"\('(\d{4}-\d{2}-\d{2})','", text))
    m = re.search(r"snapshot_date = (\d{4}-\d{2}-\d{2})", text)
    snap_date = m.group(1) if m else None
    if not snap_date:
        die("SQL から snapshot_date を読み取れない")

    # --- ファイルが途中で切れていないか ---
    #
    # 生成器がヘッダに宣言した行数と、実際に数えた VALUES の数を突き合わせる。
    # これが無いと、転送やコピーで切り詰められたファイルを黙って投入してしまう
    # (実測: 500KB に切り詰めたファイルが 19 文 / 9,192 行として受理された)。
    m2 = re.search(r"-- 書き込み行数: ([\d,]+)", text)
    if not m2:
        die(
            "ヘッダに宣言行数が無い。build_snapshot.py が生成したファイルではない可能性がある。\n"
            "  再生成: python scripts/salesnow_snapshot/build_snapshot.py "
            '--source "<csv>" --date YYYY-MM-DD'
        )
    declared = int(m2.group(1).replace(",", ""))
    if declared != rows:
        die(
            f"ファイルが不完全。ヘッダの宣言 {declared:,} 行に対し、実際の VALUES は "
            f"{rows:,} 行しかない (差 {declared - rows:,} 行)。\n"
            "  転送・コピーで切り詰められた可能性がある。再生成すること。"
        )
    if not text.rstrip().endswith(";"):
        die("SQL が `;` で終わっていない。ファイルが途中で切れている可能性がある。")

    print(f"投入対象      : {path}")
    print(f"snapshot_date : {snap_date}")
    print(f"DDL 文        : {len(ddl)}")
    print(f"INSERT 文     : {len(ins)}  (合計 {rows:,} 行)")
    print(f"Rows Written  : {rows:,} 行 = Scaler 月間 100M の "
          f"{rows / 100_000_000 * 100:.3f}%")

    # 環境変数 → 無ければ .env (アプリと同じ読み方)
    from_env_file = load_dotenv(args.env_file)
    url = os.environ.get(args.url_env, "")
    token = os.environ.get(args.token_env, "")

    def source_of(key, value):
        if not value:
            return "★未設定"
        return f"設定あり ({'.env' if key in from_env_file else '環境変数'})"

    if args.dry_run:
        print("\n[dry-run] 書き込みは行わない。")
        print(f"  {args.url_env}   : {source_of(args.url_env, url)}")
        print(f"  {args.token_env} : {source_of(args.token_env, token)}")
        if not (url and token):
            print(f"\n  {args.env_file} が見つからない場合は、リポジトリ直下に作る:")
            print(f"    {args.url_env}=libsql://<db>-<org>.turso.io")
            print(f"    {args.token_env}=<token>")
            print("  (.env は .gitignore 済み。アプリ本体も同じファイルを読む)")
        return
    if not url or not token:
        die(
            f"認証情報が無い ({args.url_env} / {args.token_env})。\n"
            f"  方法1: リポジトリ直下の {args.env_file} に書く (アプリ本体と共通)\n"
            f"    {args.url_env}=libsql://<db>-<org>.turso.io\n"
            f"    {args.token_env}=<token>\n"
            f"  方法2: 環境変数に設定する\n"
            f"    PowerShell: $env:{args.url_env}='...'; $env:{args.token_env}='...'\n"
            f"    bash      : export {args.url_env}=... {args.token_env}=...\n"
            "  取得元: Turso ダッシュボード、または Render の環境変数設定"
        )

    db = Turso(url, token)

    # --- 事前確認: 既に入っていないか (無駄な書き込みを避ける) ---
    print("\n事前確認...")
    exists = db.scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='" + TABLE + "'"
    )
    if exists:
        already = db.scalar(
            f"SELECT COUNT(*) FROM {TABLE} WHERE snapshot_date = '{snap_date}'"
        ) or 0
        total = db.scalar(f"SELECT COUNT(*) FROM {TABLE}") or 0
        print(f"  表は既にある。全体 {total:,} 行 / {snap_date} は {already:,} 行")
        if already >= rows:
            print(f"\n  この snapshot_date は投入済み。書き込まずに終了する。")
            print(f"  貼り替えるなら先に明示的に削除すること:")
            print(f"    DELETE FROM {TABLE} WHERE snapshot_date = '{snap_date}'")
            return
        if already:
            print(f"  途中まで入っている。残りだけが書き込まれる "
                  f"(INSERT OR IGNORE のため既存行は無課金)")
    else:
        print("  表はまだ無い。新規作成する。")

    # --- DDL ---
    for s in ddl:
        db.execute(s)
    print(f"  DDL {len(ddl)} 文 適用")

    # --- INSERT ---
    print(f"\n投入中 ({len(ins)} バッチ)...")
    t0 = time.perf_counter()
    for i, s in enumerate(ins, 1):
        db.execute(s)
        if i % 50 == 0 or i == len(ins):
            el = time.perf_counter() - t0
            print(f"  {i:>4}/{len(ins)}  ({i/len(ins)*100:>5.1f}%)  {el:>6.1f}s")

    # --- 事後検証 ---
    print("\n事後検証...")
    cnt = db.scalar(f"SELECT COUNT(*) FROM {TABLE} WHERE snapshot_date = '{snap_date}'")
    emp = db.scalar(
        f"SELECT SUM(employee_count) FROM {TABLE} WHERE snapshot_date = '{snap_date}'"
    )
    print(f"  行数           : {cnt:,}  (期待 {rows:,}) "
          f"{'OK' if cnt == rows else '★不一致'}")
    print(f"  従業員数合計   : {emp:,}")
    dup = db.scalar(
        f"SELECT COUNT(*) FROM (SELECT corporate_number FROM {TABLE} "
        f"WHERE snapshot_date='{snap_date}' GROUP BY 1 HAVING COUNT(*)>1)"
    )
    print(f"  法人番号の重複 : {dup}  {'OK' if dup == 0 else '★あり'}")
    print("\n完了。")


if __name__ == "__main__":
    main()
