# SURVEY MARKET INTELLIGENCE — Phase 3: e-Stat 15-1 Fetch 実装計画書

**Worker**: A4
**対象**: `scripts/fetch_estat_15_1.py` (新規作成予定、本書は計画書のみ)
**統計表 ID**: `0003454508` (statdisp_id) / API 用 `statsDataId` は `--check-meta` で確定
**データ**: 令和 2 年国勢調査 15-1 表 (男女 × 年齢 5 歳階級 × 職業大分類 × 市区町村)
**ステータス**: 計画段階。本格 fetch は appId 取得後の別ラウンド。

---

## 0. スコープ

| 項目 | 範囲 |
|------|------|
| 含む | CLI 設計、API request 設計、ページング、中断耐性、除外ルール、出力 CSV スキーマ、件数見積、検証ロジック、ロードマップ |
| 含まない | 実装コード本体、appId 取得、本格 fetch、DB 投入、Turso upload、Rust 側変更 |

---

## 1. CLI 設計

4 モードの単一スクリプト。

```
python scripts/fetch_estat_15_1.py --check-meta
    e-Stat API メタデータエンドポイントから statsDataId、総セル数、軸定義 (cat01/cat02/cat03/area の
    code/name 一覧) を取得して標準出力。1 リクエストのみ。

python scripts/fetch_estat_15_1.py --fetch [--from-page N] [--limit M] [--app-id <X>]
    ページング fetch を実行。data/generated/temp/estat_15_1_page_NNN.csv を逐次出力、
    data/generated/estat_15_1_progress.json に進捗保存。中断時は次回起動で自動再開。
    --from-page で明示再開、--limit で 1 ページ行数上書き (デフォルト 100,000)。

python scripts/fetch_estat_15_1.py --merge
    data/generated/temp/estat_15_1_page_*.csv を連結し、不詳/総数/再掲を除外、
    data/generated/estat_15_1_clean.csv を出力。

python scripts/fetch_estat_15_1.py --validate
    estat_15_1_clean.csv の整合性検証 (件数、軸完全性、JIS マスタ突合、数値妥当性)。
    終了コード 0 = pass、1 = fail。
```

排他: `--check-meta` / `--fetch` / `--merge` / `--validate` のいずれか 1 つだけ指定。

---

## 2. appId 取得・利用設計

### 2.1 取得経路 (本書スコープ外)
- e-Stat 開発者登録: https://www.e-stat.go.jp/api/
- ユーザー側で取得し、ローカル `.env` に登録。Claude は取得・閲覧しない。

### 2.2 スクリプト側受け取り
| 経路 | 優先度 | 例 |
|------|-------|-----|
| `os.getenv("ESTAT_APP_ID")` | 高 | `.env` に `ESTAT_APP_ID=xxx` |
| `--app-id <X>` CLI 引数 | 中 (env を上書き) | `python fetch_estat_15_1.py --fetch --app-id xxx` |

### 2.3 セキュリティ制約 (絶対遵守)
- **`.env` を `open()` しない** — `python-dotenv` 経由 + `os.getenv()` のみ。
- **ハードコード禁止** — ソース内に appId 文字列を書かない。
- **ログ・stdout に転記禁止** — `--app-id` で受け取った値もマスク表示。
  ```python
  def mask(s: str) -> str:
      return f"{s[:3]}*****{s[-2:]}" if s and len(s) > 5 else "*****"
  logger.info(f"appId: {mask(app_id)}")
  ```
- **本ドキュメントへの転記禁止** — 値は記載しない。
- 既存 `scripts/import_estat_labor_stats.py --app-id` パターンと整合。

---

## 3. API endpoint と request 設計

### 3.1 Endpoint
```
GET https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData
```

### 3.2 必須パラメータ
| パラメータ | 値 | 備考 |
|------------|----|----|
| `appId` | `os.getenv("ESTAT_APP_ID")` | 必須 |
| `statsDataId` | `--check-meta` で確定 | `0003454508` の API 用 ID |
| `limit` | `100000` (デフォルト) | 1 リクエスト最大 |
| `startPosition` | `1` から開始、ページごと加算 | progress.json で管理 |

### 3.3 推奨追加パラメータ
| パラメータ | 値 | 目的 |
|-----------|----|----|
| `metaGetFlg` | `Y` | 軸定義 (cat01/cat02/cat03/area) を毎ページ取得 |
| `cntGetFlg` | `N` | 件数のみモードを無効化 (本データを取得) |
| `replaceSpChars` | `2` | NULL/N.A. を空文字に置換、CSV 化のロバスト性向上 |
| `sectionHeaderFlg` | `2` | section header 抑制 |

### 3.4 レート制限
- e-Stat 公称: 1 秒 5 リクエスト程度。
- 本実装: **`time.sleep(1.0)`** を毎ページ後に固定挿入 (安全側)。
- HTTP 429 / 503 検出時は **exponential backoff** (2s → 4s → 8s → 最大 60s、5 回まで)。

---

## 4. ページング設計

```python
PAGE_SIZE = 100_000
TOTAL_ESTIMATED = 1_762_605  # 18 ページ前後

def fetch_pages(stats_data_id: str, app_id: str, page_size: int = PAGE_SIZE) -> None:
    progress = load_progress() or {
        "next_position": 1,
        "completed_pages": 0,
        "total_estimated": TOTAL_ESTIMATED,
    }
    while progress["next_position"] <= progress["total_estimated"]:
        page_num = (progress["next_position"] - 1) // page_size + 1
        json_data = api_request(
            stats_data_id, app_id,
            start=progress["next_position"], limit=page_size,
        )
        rows = parse_value_block(json_data)
        if not rows:
            break  # データ尽き
        save_page_csv(rows, f"data/generated/temp/estat_15_1_page_{page_num:03d}.csv")

        next_pos = json_data["GET_STATS_DATA"]["STATISTICAL_DATA"]\
                            ["RESULT_INF"].get("NEXT_KEY")
        if not next_pos:
            break
        progress["next_position"] = int(next_pos)
        progress["completed_pages"] = page_num
        save_progress(progress)
        time.sleep(1.0)
```

- 1 ページ ≈ 100,000 行、JSON サイズ 10–15 MB。
- 総ページ数: 18 (= ceil(1,762,605 / 100,000))。
- 想定総時間: 18 × (HTTP 5–10s + sleep 1s) ≈ 2–4 分。

---

## 5. 中断耐性

### 5.1 ファイル配置
```
data/generated/
├── estat_15_1_progress.json          ← 進捗 (next_position, completed_pages, total)
├── temp/
│   ├── estat_15_1_page_001.csv       ← 各ページ CSV (再開時はスキップ)
│   ├── estat_15_1_page_002.csv
│   ├── ...
│   └── estat_15_1_page_018.csv
├── estat_15_1_raw.csv                ← --merge 出力 (除外前)
└── estat_15_1_clean.csv              ← --merge 出力 (除外後、最終)
```

### 5.2 progress.json スキーマ
```json
{
  "next_position": 600001,
  "completed_pages": 6,
  "total_estimated": 1762605,
  "stats_data_id": "0003454508-XXXXX",
  "started_at": "2026-05-04T12:00:00+09:00",
  "last_updated_at": "2026-05-04T12:03:21+09:00"
}
```

### 5.3 再開ロジック
- 起動時に `progress.json` を読み、`next_position` から再開。
- `--from-page N` 明示時は `(N-1) * page_size + 1` で上書き。
- 各ページ書き込み完了 **後に** progress を更新 (中断で部分ページ重複を防止)。
- 既存ページ CSV があれば skip (idempotent)。

### 5.4 完走判定
```
completed_pages >= ceil(total_estimated / page_size)
AND next_position > total_estimated
AND len(glob('temp/estat_15_1_page_*.csv')) == completed_pages
```

---

## 6. 除外ルール

API レスポンスの `CLASS_INF` / `VALUE` セクションの軸コードで判定。
**実コード値は `--check-meta` で確定**。下記は推定値・暫定マッピング。

```python
# 軸 ID は e-Stat 標準命名 (cat01/cat02/cat03/area/time)
EXCLUDE_PATTERNS = {
    "cat01": ["00000"],          # 男女: 00000=総数 → 除外、1=男, 2=女 → 残す
    "cat02": ["00000", "9999"],  # 年齢階級: 00000=総数, 9999=不詳 → 除外
    "cat03": ["00000", "999"],   # 職業大分類: 00000=総数, 999=分類不能の職業 → 除外
    "area":  ["00000"],          # 地域: 00000=全国 → 除外、5 桁市区町村のみ残す
}

# 「再掲」判定: cat03 の name に「(再掲)」を含む場合は除外
RECAPTURE_NAME_PATTERN = r".*\(再掲\).*"
```

**注**: 男女別 (cat01) の `00000=総数` は通常の集計用途では有用だが、本データでは
「男 + 女」で再構成可能なため除外。残したい場合は `--keep-totals` オプションで維持可
(将来拡張、本リリースでは除外固定)。

---

## 7. 出力 CSV スキーマ (`estat_15_1_clean.csv`)

| # | カラム | 型 | 例 | 備考 |
|---|--------|-----|----|----|
| 1 | `municipality_code` | str(5) | `01101` | JIS X 0402 5 桁、ゼロパディング |
| 2 | `prefecture` | str | `北海道` | API name 由来 |
| 3 | `municipality_name` | str | `札幌市中央区` | 政令市は区まで |
| 4 | `gender` | str | `male` / `female` | 総数除外後 |
| 5 | `age_class` | str | `15-19` / `20-24` / ... / `85+` | 5 歳階級 |
| 6 | `occupation_code` | str | `A` / `B` / ... / `L` | 職業大分類 (12 区分相当、分類不能除外) |
| 7 | `occupation_name` | str | `管理的職業従事者` | API name |
| 8 | `population` | int | `1234` | 実測人数 (≥0) |
| 9 | `source_name` | str | `census_15_1` | 固定値 |
| 10 | `source_year` | int | `2020` | 令和 2 年 |
| 11 | `fetched_at` | str | `2026-05-04T12:34:56+09:00` | ISO 8601 |

**カラム数: 11**

エンコーディング: UTF-8 (BOM なし)、改行 LF。

---

## 8. 件数見積 + DB 投入見積

| 項目 | 値 |
|------|-----|
| 取得セル数 (raw) | ~1,762,605 |
| 除外後行数 | ~1,000,000 – 1,200,000 |
| 1 ページ JSON サイズ | 10 – 15 MB |
| 全 18 ページ合計 | 180 – 270 MB |
| 中間 CSV (raw) サイズ | ~150 MB |
| 最終 CSV (clean) サイズ | ~80 – 100 MB |
| SQLite テーブルサイズ | ~80 – 120 MB (1 行 ~80 bytes) |
| **Turso write 消費** | ~1.0 – 1.2M writes (月間 10M 枠の **10–12%**) |
| Turso storage 消費 | ~80 – 120 MB (5 GB 枠の 2%) |

---

## 9. 整合性検証 (`--validate`)

```python
def validate(csv_path: str) -> bool:
    df = pd.read_csv(csv_path, dtype={"municipality_code": str})

    # 9.1 行数
    assert 800_000 < len(df) < 1_500_000, f"row count out of range: {len(df)}"

    # 9.2 都道府県カバレッジ
    assert df["prefecture"].nunique() == 47

    # 9.3 市区町村カバレッジ (JIS マスタ突合)
    master_codes = load_jis_master()  # data/master/jis_municipality.csv
    csv_codes = set(df["municipality_code"].unique())
    orphan_rate = 1 - len(csv_codes & master_codes) / len(csv_codes)
    assert orphan_rate < 0.05, f"orphan municipality rate {orphan_rate:.2%}"

    # 9.4 軸完全性
    assert df["gender"].nunique() >= 2          # male/female
    assert df["age_class"].nunique() >= 14      # 5 歳 × 14 階級以上
    assert df["occupation_code"].nunique() >= 11  # 大分類 13 から総数/分類不能除外

    # 9.5 数値妥当性
    assert (df["population"] >= 0).all()
    assert df["population"].sum() > 50_000_000  # 全国就業者総数概算 (~6,700 万)

    # 9.6 ドメイン不変条件 (逆証明)
    # 市区町村別合計が 0 の行は当該市区町村に職業従事者がいない異常 → 全市区町村で >0 を期待
    muni_sum = df.groupby("municipality_code")["population"].sum()
    assert (muni_sum > 0).all(), "zero-population municipalities exist"

    # 9.7 重複チェック
    dup_keys = ["municipality_code", "gender", "age_class", "occupation_code"]
    assert not df.duplicated(subset=dup_keys).any(), "duplicate rows"

    return True
```

---

## 10. 実装ロードマップ (4 日)

| Day | 作業 | 成果物 |
|-----|------|--------|
| 1 | `--check-meta` 実装 / statsDataId 確定 / 軸コード一覧出力 / 既存 `import_estat_labor_stats.py` のパターン継承 | meta dump JSON、軸コード CSV |
| 2 | `--fetch` 実装 / progress.json / sleep 1s / 5 ページ試行 (limit=5) | temp/page_001-005.csv |
| 3 | `--merge` + `--validate` 実装 / 全 18 ページ完走 / 整合性確認 | estat_15_1_clean.csv (~1.1M 行) |
| 4 | ローカル SQLite 投入手順書 + Turso upload 手順書 (実投入はユーザー手動、Worker B4 DDL 確定後) | docs/IMPORT_PLAN.md |

---

## 11. リスクと対策

| リスク | 確率 | 影響 | 対策 |
|--------|-----|------|------|
| API レート制限 (429) | 中 | fetch 中断 | sleep 1.0s 固定 + exponential backoff (5 回) |
| appId 期限切れ / 不正 | 低 | 即時エラー | 401/403 検出時に明示メッセージ + 取得方法を stderr に案内 |
| ページング途中で API 仕様変更 | 低 | 中断 | progress.json で再開、エラー時は abort + ユーザー通知 |
| 1 ページ行数 > 100,000 | 低 | データ欠損 | レスポンスの `RESULT.STATUS` と `NEXT_KEY` で次ページ判定 |
| 1 ページの行数が 0 | 低 | 無限ループ | `if not rows: break` で離脱 |
| JSON 構造の差異 (cat01 vs catXX) | 中 | parse 失敗 | `--check-meta` で軸キーを動的に取得し parser に渡す |
| CSV エンコーディング差異 | 低 | merge 失敗 | UTF-8 固定、`replaceSpChars=2` 指定 |
| 統計表 ID `0003454508` が API 用 ID と異なる | 高 | fetch 不能 | `--check-meta` で必ず確認、本書はその段階で確定 |

---

## 12. 承認待ち事項

| # | 事項 | 確認先 |
|---|------|--------|
| 1 | appId 取得タイミング (本タスクでは取得しない、計画 + DDL 確定後にユーザーが `.env` 設定) | ユーザー |
| 2 | データソースラベル `census_15_1` の正式名称承認 (代替案: `kokusei_15_1_2020`) | ユーザー |
| 3 | ローカル DB 投入後の Turso upload タイミング | Worker B4 DDL 確定後 |
| 4 | `municipality_name` の政令市区表記 (「札幌市中央区」 vs 「札幌市」+「中央区」分割) | ユーザー |
| 5 | `age_class` の表記 (`15-19` vs `15〜19歳` vs `15_19`) | スキーマ整合性で `A-B` 形式推奨 |

---

## 13. 制約 (本タスク遵守事項)

- 本格 fetch 不実施 (本書は計画書のみ)
- DB 書き込み禁止
- Turso 接続不要
- `.env` 直接 open 禁止
- Rust 変更禁止
- push 禁止
- 新規ファイル: 本書 docs 1 つのみ (実装スクリプトは Phase 5 別ラウンド)

---

## 14. 参照

- 既存パターン: `scripts/import_estat_labor_stats.py` (e-Stat API 接続)
- 既存パターン: `scripts/fetch_industry_structure.py` (e-Stat fetch + CSV 出力)
- 既存パターン: `scripts/fetch_commute_od.py` (47 県逐次 fetch + 中断耐性)
- 先行調査: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_ESTAT_15_1_FEASIBILITY.md` (Worker D3)
- e-Stat API 仕様: https://www.e-stat.go.jp/api/api-info/api-spec
