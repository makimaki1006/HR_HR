# -*- coding: utf-8 -*-
"""企業ジオコードデータをTurso (SalesNow DB) にアップロードする。

使い方:
    python upload_company_geocode_to_turso.py --url URL --token TOKEN
    python upload_company_geocode_to_turso.py --dry-run
    python upload_company_geocode_to_turso.py --resume  (中断時の再開)

環境変数でも指定可能:
    SALESNOW_TURSO_URL, SALESNOW_TURSO_TOKEN

最適化 (2026-04-04):
    - バッチサイズ 200 → 500（Turso pipeline上限~1000に対して安全マージン）
    - BEGIN/COMMIT トランザクション包囲で書き込み効率向上
    - ThreadPoolExecutor(3 workers) による並列アップロード
    - 推定速度改善: 逐次1,188req → 並列158req（約5-7倍高速化）
"""
import csv
import json
import os
import sys
import time
import argparse
import threading
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
    import requests
except ImportError:
    print("requests が必要: pip install requests")
    sys.exit(1)

SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR.parent / "data"
CSV_FILE = DATA_DIR / "company_geocode.csv"

# バッチサイズ: Turso pipeline は最大~1000 statements/request
# BEGIN + 500 INSERT + COMMIT = 502 statements で安全マージン確保
BATCH_SIZE = 500

# 並列ワーカー数: Tursoの同時接続制限を考慮して3に設定
MAX_WORKERS = 3

SCHEMA = """
    CREATE TABLE IF NOT EXISTS v2_company_geocode (
        corporate_number TEXT PRIMARY KEY,
        lat REAL NOT NULL,
        lng REAL NOT NULL,
        geocode_source TEXT DEFAULT 'postal_centroid',
        geocode_confidence INTEGER DEFAULT 1
    )
"""

# スレッドセーフなカウンター
_lock = threading.Lock()
_uploaded_count = 0
_error_count = 0


def turso_pipeline(url, token, statements, timeout=120):
    """Turso HTTP Pipeline APIで複数SQLを一括実行"""
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }

    requests_list = []
    for sql, params in statements:
        stmt = {"sql": sql}
        if params:
            stmt["args"] = [
                {"type": "null", "value": None} if v is None
                else {"type": "integer", "value": str(v)} if isinstance(v, int)
                else {"type": "float", "value": v} if isinstance(v, float)
                else {"type": "text", "value": str(v)}
                for v in params
            ]
        requests_list.append({"type": "execute", "stmt": stmt})

    requests_list.append({"type": "close"})

    resp = requests.post(
        f"{url}/v2/pipeline",
        headers=headers,
        json={"requests": requests_list},
        timeout=timeout,
    )

    if resp.status_code != 200:
        raise Exception(f"Turso API error {resp.status_code}: {resp.text[:300]}")

    data = resp.json()
    errors = [r for r in data.get("results", []) if r.get("type") == "error"]
    if errors:
        raise Exception(f"SQL errors: {errors[:3]}")

    return data


def _wrap_batch_in_transaction(insert_stmts):
    """INSERT文のリストをBEGIN/COMMITトランザクションで包む。

    トランザクションにより個別コミットが不要になり、
    ディスクI/O回数が大幅に削減される。
    """
    wrapped = [("BEGIN", [])]
    wrapped.extend(insert_stmts)
    wrapped.append(("COMMIT", []))
    return wrapped


def _upload_batch(url, token, batch, batch_idx):
    """1バッチをアップロード（スレッド内で実行）。

    リトライロジック: 一時的なネットワークエラーに対し最大2回リトライ。
    """
    global _uploaded_count, _error_count
    max_retries = 2
    stmts = _wrap_batch_in_transaction(batch)

    for attempt in range(max_retries + 1):
        try:
            # バッチサイズに応じてタイムアウトを調整
            timeout = max(120, len(batch) // 2)
            turso_pipeline(url, token, stmts, timeout=timeout)
            with _lock:
                _uploaded_count += len(batch)
            return True
        except Exception as e:
            if attempt < max_retries:
                # 指数バックオフ: 1秒, 2秒
                time.sleep(1 * (attempt + 1))
                continue
            # 最終リトライも失敗
            with _lock:
                _error_count += len(batch)
            print(f"  バッチ#{batch_idx} エラー ({len(batch)}件): {e}")
            return False


def upload_geocode(turso_url, turso_token, dry_run=False, resume=False):
    """CSVから企業ジオコードデータをTursoにアップロード"""
    global _uploaded_count, _error_count
    _uploaded_count = 0
    _error_count = 0

    if not CSV_FILE.exists():
        print(f"エラー: CSVが見つかりません: {CSV_FILE}")
        sys.exit(1)

    if not resume:
        print("テーブル作成...")
        stmts = [
            ("DROP TABLE IF EXISTS v2_company_geocode", []),
            (SCHEMA, []),
        ]
        if not dry_run:
            turso_pipeline(turso_url, turso_token, stmts)
        print("  v2_company_geocode: 作成完了")

    # CSV読み込み + バッチ構築
    print(f"\nCSV読込: {CSV_FILE}")
    print(f"設定: バッチサイズ={BATCH_SIZE}, 並列ワーカー={MAX_WORKERS}")

    # 全行を先にパースしてバッチリストを作成
    all_batches = []
    current_batch = []
    total = 0
    skipped = 0

    with open(CSV_FILE, encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            corp = row.get("corporate_number", "").strip()
            lat = row.get("lat", "").strip()
            lng = row.get("lng", "").strip()
            source = row.get("geocode_source", "postal_centroid").strip()
            confidence = row.get("geocode_confidence", "1").strip()

            if not corp or not lat or not lng:
                skipped += 1
                continue

            try:
                lat_f = float(lat)
                lng_f = float(lng)
                conf_i = int(confidence)
            except ValueError:
                skipped += 1
                continue

            sql = (
                "INSERT OR REPLACE INTO v2_company_geocode "
                "(corporate_number, lat, lng, geocode_source, geocode_confidence) "
                "VALUES (?1, ?2, ?3, ?4, ?5)"
            )
            current_batch.append((sql, [corp, lat_f, lng_f, source, conf_i]))
            total += 1

            if len(current_batch) >= BATCH_SIZE:
                all_batches.append(current_batch)
                current_batch = []

    # 残りのバッチ
    if current_batch:
        all_batches.append(current_batch)

    num_batches = len(all_batches)
    print(f"  有効行数: {total}, スキップ: {skipped}")
    print(f"  バッチ数: {num_batches} (各 最大{BATCH_SIZE}行)")

    if dry_run:
        print(f"\n完了 (ドライラン):")
        print(f"  総行数: {total}")
        print(f"  バッチ数: {num_batches}")
        print(f"  推定リクエスト数: {num_batches} (旧: {(total + 199) // 200})")
        print(f"  実際のアップロードなし")
        return

    # 並列アップロード
    start_time = time.time()
    print(f"\nアップロード開始 ({MAX_WORKERS}並列)...")

    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
        futures = {}
        for idx, batch in enumerate(all_batches):
            future = executor.submit(_upload_batch, turso_url, turso_token, batch, idx)
            futures[future] = idx

        # 進捗表示
        completed = 0
        for future in as_completed(futures):
            completed += 1
            if completed % 20 == 0 or completed == num_batches:
                elapsed = time.time() - start_time
                rate = _uploaded_count / elapsed if elapsed > 0 else 0
                eta = (total - _uploaded_count) / rate if rate > 0 else 0
                print(
                    f"  進捗: {completed}/{num_batches}バッチ "
                    f"({_uploaded_count}行完了, "
                    f"{rate:.0f}行/秒, "
                    f"残り{eta:.0f}秒)"
                )

    elapsed = time.time() - start_time
    print(f"\n完了: {elapsed:.1f}秒")
    print(f"  総行数: {total}")
    print(f"  アップロード: {_uploaded_count}")
    print(f"  エラー: {_error_count}")
    if elapsed > 0:
        print(f"  スループット: {_uploaded_count / elapsed:.0f} 行/秒")


def main():
    parser = argparse.ArgumentParser(description="企業ジオコードをTursoにアップロード")
    parser.add_argument("--url", default=os.environ.get("SALESNOW_TURSO_URL", ""), help="Turso URL")
    parser.add_argument("--token", default=os.environ.get("SALESNOW_TURSO_TOKEN", ""), help="Turso token")
    parser.add_argument("--dry-run", action="store_true", help="実行せずに確認のみ")
    parser.add_argument("--resume", action="store_true", help="テーブル再作成せずに追加")
    args = parser.parse_args()

    if not args.dry_run and (not args.url or not args.token):
        print("エラー: --url と --token が必要です（環境変数 SALESNOW_TURSO_URL/TOKEN でも可）")
        sys.exit(1)

    upload_geocode(args.url, args.token, dry_run=args.dry_run, resume=args.resume)


if __name__ == "__main__":
    main()
