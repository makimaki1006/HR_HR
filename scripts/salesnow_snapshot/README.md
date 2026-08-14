# 従業員数スナップショット (Turso 投入手順)

## 何のために

人員推移をパーセントからの逆算ではなく、**従業員数そのものの定点観測**で持つ。

現在は `employee_delta_*` (対過去比パーセント) しか無く、増減人数を
`employee_count × d / (100 + d)` で復元している。この式には次の問題がある
(実測は [`claudedocs/SALESNOW_MAP_PHASE0_FINDINGS_2026-08-12.md`](../../claudedocs/SALESNOW_MAP_PHASE0_FINDINGS_2026-08-12.md) 参照)。

| 現在の問題 | スナップショットが揃った後 |
|---|---|
| `d → -100` で発散する極。実データに 188 社 (§11.5) | **消える**。引き算になる |
| `delta` が小数第 2 位までのため復元値に最大 ±281 人の不確かさ (§11.5) | **消える**。実測値どうしの差 |
| `collated_at` が 244 日に分散し「1 年前」が企業ごとに違う (§4) | **消える**。`snapshot_date` で揃う |
| 記録の付け替え (東芝 -5,187 / 東芝 +8,732) が雇用減に見える | **見えるようになる**。段差として検出できる |

⚠ **粒度不足で 1 社が符号を決める問題 (§11.5 の問題 B) は、これでは直らない。**
都道府県 × 業種で母数が足りないという別問題であり、抑制ゲートで対処する。

## 制限とコスト (Turso Scaler)

| 項目 | 1 回のスナップショット | Scaler の枠 | 使用率 |
|---|---|---|---|
| Rows Written | 212,856 行 | 100 M / 月 | **0.213%** |
| Storage | 10.4 MB | 24 GB | 0.04% |

月次で持ち続けた場合の累積:

| 期間 | Storage | 24GB に対して |
|---|---|---|
| 1 年 (12 回) | 125 MB | 0.5% |
| 3 年 (36 回) | 375 MB | 1.5% |
| 5 年 (60 回) | 625 MB | 2.6% |

⚠ **他の用途と共有しているため、投入前に残枠を確認すること。**

```bash
turso db inspect salesnow
turso db inspect country-statistics   # v2_flow_mesh1km_* が 38M 行ある
```

## 手順

⚠ **以下のコマンドはすべてリポジトリのルートで実行する。**
相対パス (`scripts/salesnow_snapshot/...`) はルート基準で書いてある。

```bash
cd C:/Users/fuji1/AppData/Local/Temp/HR_HR_salesnow_map
```

生成物は `scripts/salesnow_snapshot/out/` に出る (`.gitignore` 済み、約 21MB)。
**別の場所に `--outdir` を向けた場合は、以降のコマンドのパスも合わせて変えること。**

### 1. スナップショット SQL を生成する (私が実行可)

```bash
python scripts/salesnow_snapshot/build_snapshot.py \
    --source "C:/Users/fuji1/OneDrive/デスクトップ/HR_HR/data/salesnow_companies.csv" \
    --date 2026-08-12 \
    --outdir scripts/salesnow_snapshot/out
```

生成されるもの:
- `out/snapshot_YYYY-MM-DD.csv` — 目視確認用 (4 列)
- `out/snapshot_YYYY-MM-DD.sql` — 投入用 (約 11 MB)

生成時に次を標準出力へ出す。**投入前に必ず読むこと。**
- 書き込む行数と、月間枠に対する割合
- 法人番号が無くて除外した行数
- 重複を集約した行数

### 2. 投入する (ユーザーが実行)

⚠ **DB 書き込みはユーザー実行のみ** (`feedback_turso_priority`、2026-01 $195 超過請求)。

`turso` CLI は**使わない**。理由は 2 つ:

- PowerShell では `<` が予約語で、`turso db shell db < file.sql` は
  `RedirectionNotSupported` で失敗する
- そもそも CLI の導入が必要になる

アプリ本体 (`src/db/turso_http.rs`) と同じ libSQL HTTP API を叩く投入スクリプトを使う。
CLI もリダイレクトも要らない。

#### 認証情報の置き場所

アプリ本体は `main.rs:13` で `dotenvy::dotenv()` を呼び、**リポジトリ直下の `.env`**
から読む。投入スクリプトも同じファイルを読むので、置き場所は 1 箇所で済む。

`.env` (`.gitignore` 済み):

```
SALESNOW_TURSO_URL=libsql://<db>-<org>.turso.io
SALESNOW_TURSO_TOKEN=<token>
```

`libsql://` でも `https://` でも受ける。取得元は Turso ダッシュボード、
または Render (`hellowork-dashboard`) の環境変数設定。

Windows で作ったファイルでも読めるよう、次を吸収する (実測で検証済み):

| 書き方 | 読める |
|---|---|
| CRLF 改行 (メモ帳の既定) | OK |
| **UTF-8 BOM 付き** (メモ帳の既定) | OK |
| `export KEY=VALUE` (bash 例のコピペ) | OK |
| `"値"` のクォート囲み / 前後の空白 | OK |
| 値に `=` を含む (JWT のパディング) | OK |
| `$env:KEY = "値"` (PowerShell 形式) | 読めない → **警告を出す** |
| `SALESNOW_TURSO_*` が無い | 読めたキー名を**警告に出す** |

BOM は「`SALESNOW_TURSO_URL` だけ未設定、TOKEN は設定あり」という
分かりにくい壊れ方をするため、明示的に対応している。

環境変数を直接設定してもよい。**環境変数が `.env` より優先される** (dotenvy と同じ)。

```powershell
$env:SALESNOW_TURSO_URL   = "libsql://..."
$env:SALESNOW_TURSO_TOKEN = "..."
```

#### 実行

```powershell
# 何が起きるかだけ見る (書き込まない)。認証情報の読み取り元も表示される
python scripts/salesnow_snapshot/import_snapshot.py --dry-run

# 投入
python scripts/salesnow_snapshot/import_snapshot.py
```

`--dry-run` は値そのものを出さず、`設定あり (.env)` / `設定あり (環境変数)` /
`★未設定` だけを表示する。

投入スクリプトの安全設計 (libSQL 互換スタブ相手に実測済み):

| 状況 | 動作 | 終了コード |
|---|---|---|
| 同じ `snapshot_date` が投入済み | **書き込まずに終了**。貼り替え用の `DELETE` を提示 | 0 |
| 途中で落ちた後の再実行 | 入っている分を検出し**残りだけ**投入 (162,856 → 残り 50,000 行) | 0 |
| **SQL が途中で切れている** | ヘッダの宣言行数と実測を突き合わせて**中止** | 1 |
| 認証情報が無い / 片方だけ | 設定方法を示して中止。接続しに行かない | 1 |
| 接続できない | HTTP エラーを表示して中止。書き込みは発生しない | 1 |
| 想定外の文が SQL に混ざる | 投入前に検出して中止 | 1 |

⚠ **切り詰め検査を入れた経緯**: 初版は 500KB に切り詰めたファイルを
「19 文 / 9,192 行」として黙って受理していた。単体では矛盾が無いため気づけない。
ヘッダの `-- 書き込み行数:` と実際の VALUES 数を突き合わせて検出する。
`verify_snapshot_waste.py` にも同じ検査を入れてある。

投入後は行数・従業員数合計・法人番号の重複を自動で読み出して検証する。

<details>
<summary>turso CLI を使う場合 (PowerShell の正しい書き方)</summary>

```powershell
# `<` は使えない。Get-Content でパイプする
Get-Content -Raw scripts/salesnow_snapshot/out/snapshot_2026-08-12.sql | turso db shell salesnow
```

ただし CLI 経路では上記の事前確認・事後検証が働かない。
</details>

### 3. 確認する

`import_snapshot.py` が投入直後に自動で実行する。手動で確認するなら:

```powershell
python scripts/salesnow_snapshot/import_snapshot.py --dry-run   # 再実行しても安全
```

2026-08-12 のスナップショットなら **212,856 行 / 22,666,055 人**、法人番号の重複 0 になる
(ローカル SQLite と libSQL 互換スタブの両方で確認済み)。

## 冪等性と再試行コスト (実測)

`CREATE TABLE IF NOT EXISTS` + `INSERT OR IGNORE`。`DROP` は一切しない
(`feedback_turso_upload_once`、2026-04-03 無料枠浪費)。

`OR REPLACE` ではなく `OR IGNORE` を選んだのは課金のため。
SQLite の `total_changes` で実測した:

| 方式 | 誤って再実行 | 途中失敗後の再試行 |
|---|---|---|
| `INSERT OR REPLACE` | 212,856 行を再課金 | 212,856 行 (全額) |
| **`INSERT OR IGNORE`** | **0 行** | **162,856 行** (残りのみ) |

スナップショットは不変の記録なので上書き意味論は要らない。
値が誤っていた場合は別の `snapshot_date` を使う。同じ日付を意図的に貼り替えるには
先に `DELETE FROM ... WHERE snapshot_date = '...'` を明示すること
(黙って歴史を書き換えないための設計)。

### 投入前の検証

```bash
# 引数なし: out/ の最新 snapshot_*.sql を自動で拾う
python scripts/salesnow_snapshot/verify_snapshot_waste.py

# ファイルを明示することもできる
python scripts/salesnow_snapshot/verify_snapshot_waste.py path/to/snapshot_2026-08-12.sql
```

対象が見つからない場合は生成コマンドを表示して異常終了する (exit 1)。

次を実測する。**課金される DB に流す前に必ず通すこと。**

| 検査 | 2026-08-12 の結果 |
|---|---|
| 破壊的文 (DROP/DELETE/UPDATE/ALTER) | **0 件** |
| ファイル内の PK 重複 (自己 REPLACE) | **0 件** |
| 1 回目の書き込み | 212,856 行 (増幅率 **1.00 倍**) |
| 2 回目の書き込み | **0 行** |
| PK 重複 / 法人番号 NULL / 日付混在 / 負の従業員数 | すべて 0 |

⚠ 索引は 2 個 (PRIMARY KEY 由来 + 明示的な 1 個) 付く。
**Turso が索引への書き込みを行数としてどう数えるかは確認できていない。**
最悪 3 倍を見込むと 638,568 行相当で、Scaler 100M/月に対し 0.639%。

## 前処理として何をしているか

| 処理 | 件数 | 理由 |
|---|---|---|
| 法人番号が無い行を除外 | 234 行 | 主キーを作れない。全行 `employee_count` も NULL |
| 重複法人を最新 `collated_at` に集約 | 55 行 | 主キー衝突。54 法人すべて `collated_at` が異なるため一意に決まる |

後者は `claudedocs/.../§3` が「地域集計時に二重計上されるため前処理が必要」と
指摘していた処理そのもの。スナップショット側で解消される。

## 運用上の注意

**1 回目は「基準点」であって時系列ではない。** 中身は `collated_at` が
2025-11〜2026-07 に散らばった値であり、真の増減が取れるのは 2 回目以降。

`collated_at` を列として残しているのは、「スナップを取った日」と
「その値が実際に収集された日」を混同しないため。244 日の分散はこの列で可視化できる。

## 揃った後にどう使うか (未実装)

```sql
-- 2 時点の差から実際の増減人数を出す (逆算しない)
SELECT a.corporate_number,
       b.employee_count - a.employee_count AS change
FROM v2_salesnow_headcount_snapshot a
JOIN v2_salesnow_headcount_snapshot b USING (corporate_number)
WHERE a.snapshot_date = '2026-08-12' AND b.snapshot_date = '2027-08-12'
```

切り替えは `handlers::region_headcount` の入力を差し替えるだけで済むよう、
現在の実装は `HeadcountAggregate::from_parts` で人数を受ける形にしてある。
