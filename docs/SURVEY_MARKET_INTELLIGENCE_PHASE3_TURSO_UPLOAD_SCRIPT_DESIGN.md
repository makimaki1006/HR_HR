# Phase 3 Step 5 Turso Upload スクリプト改修設計案

**作成**: 2026-05-04 / Worker X2
**範囲**: 設計のみ。実装は別ラウンド。
**前提**: Worker X1 のローカル/Turso 差分調査結果を併用する想定。

---

## 1. 既存スクリプト分析

調査対象: `scripts/upload_to_turso.py` / `scripts/upload_new_external_to_turso.py` / `scripts/upload_salesnow_to_turso.py`

| 観点 | upload_to_turso.py | upload_new_external_to_turso.py | upload_salesnow_to_turso.py |
|---|---|---|---|
| 接続 | `requests` + `/v2/pipeline` HTTP | 同左 | 同左 |
| 入力 | ローカル SQLite (hellowork.db) | CSV (`data/`, `data/output/`) | CSV (`data/salesnow_companies.csv`) |
| DDL 適用 | **常に DROP+CREATE** (強制 replace) | `--refresh` で DROP+CREATE、デフォルトは `CREATE IF NOT EXISTS` | DROP+CREATE (resume 時のみ skip) |
| バルク INSERT | pipeline `INSERT OR REPLACE` バッチ 200 | バッチ 500 + `BEGIN/COMMIT` 包含 | バッチ 500 + `BEGIN/COMMIT` + 3 並列 + 3 retry |
| 進捗表示 | 1000 行ごと print | 1000 行ごと print | グローバルカウンタ |
| エラー処理 | 例外を catch して継続 (テーブル単位) | 同左 | バッチ単位 retry × 3、失敗カウント保持 |
| 冪等性 | DROP+CREATE のため毎回真新規 | `--refresh` で同左、デフォルトは `INSERT OR REPLACE` で冪等 | resume サポートあり |
| 安全装置 | なし (token/url 未設定で abort のみ) | dry-run、`--refresh` 明示要 | dry-run、resume |
| 認証 | CLI または `TURSO_EXTERNAL_*` env | `.env` 自動読込 + `TURSO_EXTERNAL_*` | `SALESNOW_TURSO_*` |
| dry-run | 行数表示のみ | CSV パース + バリデーション込み | フル準備のみ実行 |
| verify | 全テーブル `COUNT(*)` + 千代田区サンプル | 行数のみ | 別 verify スクリプトに分離 |

**強み・弱みサマリ**

- `upload_to_turso.py`: シンプル / 強制 replace で**事故リスク高** (誤実行で全消失) / 安全装置ゼロ。
- `upload_new_external_to_turso.py`: `--refresh` 任意化、`.env` 自動読込、追加インデックス対応で**最も Phase 3 Step 5 に近い構造**。SSDSE-A 拡張の実績あり。
- `upload_salesnow_to_turso.py`: バッチ retry/並列が**唯一実装済**。ただし用途が SalesNow 専用で密結合。

---

## 2. 推奨改修方針 (3 案比較)

| 案 | 内容 | メリット | デメリット |
|---|---|---|---|
| A. `upload_to_turso.py` 拡張 | TABLES/SCHEMAS に Phase 3 Step 5 を追加し mode 増設 | 1 ファイル管理、verify 機能流用 | 強制 replace ロジックを大改修必要、巨大化、既存運用 (ts_* 等) と衝突リスク |
| **B. 専用スクリプト新規作成 (`scripts/upload_phase3_step5.py`)** | Phase 3 Step 5 の 7 テーブル専用、`upload_new_external_to_turso.py` を骨格に派生 | 既存に影響ゼロ、レビュー範囲明確、安全装置を新設しやすい、ロールバック専用化可 | スクリプト数+1 (許容範囲) |
| C. テーブル個別スクリプト | 7 ファイル | 最大独立性 | DDL/接続/CLI のコピペ重複、保守地獄 |

### 推奨: **案 B (専用スクリプト新規作成)**

理由:
1. Phase 3 Step 5 は 7 テーブルが designated_ward (1,750 行) / 新規 (target_thickness, occupation) / skip 候補 (commute_flow_*) を**混在**しており、テーブル別 strategy 制御が必須。既存スクリプト拡張だと条件分岐が爆発する。
2. 案 A の `upload_to_turso.py` は強制 replace 設計で、Phase 3 Step 5 の incremental 用途と相性が悪い。
3. `upload_new_external_to_turso.py` から DDL 抽出・`.env` 読込・retry を流用すれば実装コスト最小。
4. ロールバック・max-writes など Phase 3 Step 5 特有の安全装置を専用化できる。

---

## 3. 推奨案の詳細設計 (`scripts/upload_phase3_step5.py`)

### 3.1 CLI 構造

```python
mode = parser.add_mutually_exclusive_group(required=True)
mode.add_argument("--dry-run",     action="store_true")
mode.add_argument("--check-remote",action="store_true")
mode.add_argument("--upload",      action="store_true")
mode.add_argument("--verify",      action="store_true")
mode.add_argument("--rollback",    action="store_true")

parser.add_argument("--tables", nargs="+", default=None,
                    help="対象テーブル絞込 (default: 全 7)")
parser.add_argument("--strategy", choices=["replace", "incremental", "auto"],
                    default="auto",
                    help="auto: テーブル別の推奨を適用 (3.2 参照)")
parser.add_argument("--batch-size", type=int, default=500)
parser.add_argument("--max-writes", type=int, default=850_000,
                    help="安全装置: 投入合計がこれを超えたら abort")
parser.add_argument("--yes", action="store_true",
                    help="確認プロンプトを skip (CI 用)")
parser.add_argument("--db", default="data/hellowork.db")
```

### 3.2 メインフロー

```
load_env(.env)
local = sqlite3.connect(args.db, mode="ro")
turso_url, turso_token = env or sys.exit
[--upload] confirm prompt unless --yes

For each target table:
  1. local_count   = SELECT COUNT(*) FROM table
  2. local_columns = PRAGMA table_info(table)
  3. remote_exists, remote_count = check_remote(table)
  4. resolved_strategy = TABLE_STRATEGY[table] if strategy=="auto" else strategy
  5. estimate_writes (3.4)
  6. if total > max-writes: abort
  7. switch mode:
     dry-run     -> print plan only
     check-remote-> print remote_exists/remote_count/diff vs local
     upload      -> apply DDL → bulk INSERT (batch + BEGIN/COMMIT + retry x3)
     verify      -> remote_count == local_count かつ sample 5 行 hash 比較
     rollback    -> 3.5 参照

post-upload:
  全テーブル verify を自動実行
  サマリ JSON を `data/output/phase3_step5_upload_summary.json` に保存
```

### 3.3 安全装置 (一覧)

1. `--max-writes` で投入総数ガード (default 850,000)。
2. `--upload` 開始前に既存行数+消失行数を表示し、`--yes` なしなら **interactive 確認**。
3. `--strategy=replace` 適用テーブルがある場合、DDL 実行前に「DROP しますがよいですか?」と再確認。
4. token / url を log 出力時 `***` でマスク。
5. 連続バッチエラー 5 回で **abort** (1 バッチあたり retry 3 回までは継続)。
6. ローカル DB は `mode=ro` で open (誤書き込み防止)。
7. `verify` で行数差が 1 行でもあれば exit-code 非 0。
8. summary JSON に upload した SQL 件数・writes 見積・実行時間を残す (監査用)。
9. `prepush_guard.py` (Worker A 既存) が想定する命名規則 (`scripts/upload_*.py`) に従う。
10. Turso 単一エンドポイント (`TURSO_EXTERNAL_URL`) のみ使用、間違って V1 の URL を渡されたらホスト名 allowlist で reject。

### 3.4 row writes 見積ロジック

```python
def estimate_writes(table, strategy):
    n = local_count(table)
    if strategy == "replace":
        # DDL は writes 計上外 (Turso 課金は INSERT/UPDATE/DELETE 行)
        return n  # 純 INSERT n 行
    if strategy == "incremental":
        # INSERT OR REPLACE は 1 行 = 1 write
        return n
    if strategy == "skip":
        return 0
```

集計後に「合計見積 = X writes、Turso 残枠 = ? 」を表示。

### 3.5 ロールバック設計

```python
def rollback(table, strategy, ref):
    # strategy=replace でリリース直後に revert したい:
    #   Turso 上の table を DROP + 元の DDL 再作成 (空テーブル)
    #   → 元データ復旧不可。ローカルから再 upload で復元可能。
    # strategy=incremental:
    #   ref で指定された source_year / municipality_code 範囲のみ DELETE
    #   ref が無ければ rollback 拒否 (全 DELETE は禁止)
```

`--rollback` は **必ず** `--tables` と `--ref KEY=VAL` の併用必須にし、誤爆を防ぐ。

---

## 4. テーブル別 strategy 推奨 (Worker X1 結果を待つ前提の暫定)

| テーブル | 推奨 strategy | 想定 writes | 理由 |
|---|:---:|---:|---|
| `v2_external_population` | **incremental** | ~175 | 既存 1,742 行を保持し designated_ward 175 件を upsert |
| `v2_external_population_pyramid` | **incremental** | ~1,575 | 同上、PK (prefecture, municipality, age_group) で重複回避 |
| `municipality_occupation_population` | **replace** | ~?? | 新規テーブル、ローカル完成済 |
| `v2_municipality_target_thickness` | **replace** | ~?? | 新規 |
| `municipality_code_master` | **replace** or **skip** | 1,747 | Worker X1 が「既存と一致」を確認したら skip |
| `commute_flow_summary` | **skip (default)** | 0 | 既存と同等想定。差分が出れば incremental に格上げ |
| `v2_external_commute_od_with_codes` | **skip (default)** | 0 | 同上 |

合計見積 (上限ケース): replace 適用 3 テーブル + incremental 1,750 = **概ね 50k writes 以内**。`--max-writes 850000` で十分カバー。

---

## 5. テスト戦略 (実 upload 前)

1. `python scripts/upload_phase3_step5.py --dry-run` → ローカル読込 + 投入計画表示 (Turso 接続なし)。
2. `python scripts/upload_phase3_step5.py --check-remote` → 既存 Turso 状態を READ-only で確認。
3. `python scripts/upload_phase3_step5.py --upload --tables v2_municipality_target_thickness --yes` → 新規 1 テーブル先行投入。
4. `python scripts/upload_phase3_step5.py --verify --tables v2_municipality_target_thickness` で整合性確認。
5. PASS なら残り 6 テーブルを順次 (まず replace 系、次に incremental 系)。
6. 最後に `--verify` 全テーブル実行、summary JSON を保存して終了。

---

## 6. ハードコード禁止項目

| 項目 | ルール |
|---|---|
| `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN` | 環境変数経由のみ。コード内に文字列禁止 |
| `.env` open | `load_env(ENV_FILE)` ヘルパ流用、直接 `open(".env")` 禁止 |
| テーブル名 / カラム / DDL | ローカル DB から `PRAGMA table_info` + `sqlite_master` で動的取得、辞書持ちは TABLE_STRATEGY のみ |
| max-writes / batch-size | CLI 引数経由 |
| host allowlist | env 経由でなく定数化可 (security 用途) |

---

## 7. 実装ロードマップ (本タスク後の別ラウンド)

| Phase | 作業 | 所要 |
|---:|---|:---:|
| 1 | Worker X1 差分調査結果反映 (TABLE_STRATEGY 確定) | 即時 |
| 2 | スクリプト本実装 (案 B、`upload_new_external_to_turso.py` 派生) | 1 日 |
| 3 | `--dry-run` / `--check-remote` 動作確認 | 0.5 日 |
| 4 | テスト upload (1 テーブル先行、ユーザー手動) | ユーザー |
| 5 | 本格 upload (残り 6 テーブル、ユーザー手動) | ユーザー |
| 6 | `--verify` 整合性確認 + summary JSON commit | 0.5 日 |

**合計工数**: Claude 実装側 = **2 日**、ユーザー手動 upload = 別途 (Turso 残枠次第)。

---

## 8. 既存資産の流用ポイント

| 流用元 | 流用内容 |
|---|---|
| `upload_new_external_to_turso.py` | `turso_pipeline()` / `load_env()` / `_cast_value()` / verify ロジック |
| `upload_salesnow_to_turso.py` | バッチ retry × 3、`BEGIN/COMMIT` 包含、エラーカウンタ |
| `upload_to_turso.py` | サンプル検証パターン (千代田区相当を `municipality_code_master` で適用) |

**禁止**: 上記 3 ファイルは本ラウンドで**変更しない**。新スクリプトに関数をコピーする方針。

---

## 9. 未確定事項 (要 Worker X1 連携)

- 各テーブルのローカル/Turso 行数差分。
- `municipality_code_master` を skip するか replace するかの最終判断。
- `commute_flow_summary` / `v2_external_commute_od_with_codes` の Turso 既存有無。

これら確定後、TABLE_STRATEGY と max-writes を最終調整する。
