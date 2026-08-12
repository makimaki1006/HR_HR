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

```bash
turso db shell salesnow < scripts/salesnow_snapshot/out/snapshot_2026-08-12.sql
```

### 3. 確認する

```bash
turso db shell salesnow \
  "SELECT snapshot_date, COUNT(*) AS rows, SUM(employee_count) AS total_emp
   FROM v2_salesnow_headcount_snapshot GROUP BY snapshot_date ORDER BY snapshot_date"
```

2026-08-12 のスナップショットなら `212856` 行 / `22666055` 人 になる
(ローカル SQLite で検証済み)。

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
python scripts/salesnow_snapshot/verify_snapshot_waste.py
```

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
