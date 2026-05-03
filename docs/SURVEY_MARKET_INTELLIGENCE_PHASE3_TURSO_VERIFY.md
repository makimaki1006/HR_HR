# Phase 3 前処理 (c): Turso V2 同期検証 実行手順書

作成日: 2026-05-04
対象: ハローワーク分析システムV2 / `data/hellowork.db` ↔ Turso V2

---

## 1. 目的

Phase 3 着手前に、ローカル `data/hellowork.db` と本番 Turso V2 (`country-statistics-makimaki1006.aws-ap-northeast-1.turso.io`) の **テーブル存在 / 行数 / 代表値** を比較し、差分を可視化する。

MEMORY ルール「Turso優先」(ローカル DB 更新だけでは本番に反映されない) に基づき、本番反映状態を可視化することで Phase 3 実装時の前提齟齬を防ぐ。

---

## 2. 安全装置 (READ-ONLY 保証)

### 2.1 設計原則

| 項目 | 内容 |
|------|------|
| 許可 SQL | `SELECT`, `PRAGMA`, テーブル名検証用 `sqlite_master` 参照のみ |
| 禁止 SQL | `INSERT`, `UPDATE`, `DELETE`, `DROP`, `CREATE`, `ALTER`, `TRUNCATE`, `REPLACE`, `ATTACH`, `DETACH`, `VACUUM`, `REINDEX`, `GRANT`, `REVOKE` |
| 検出方法 | 全 SQL を実行前に正規表現 (`FORBIDDEN_REGEX`) でチェック → 違反時 `ReadOnlyViolation` raise → 即終了 |
| READ 上限 | デフォルト 100 (Turso 無料枠 300/月の 1/3、安全マージン) |
| 上限到達時 | `ReadLimitExceeded` raise → 残りテーブルを `READ_LIMIT` ステータスでスキップ → 部分レポート出力 |
| ローカル DB | URI モード `mode=ro` で read-only オープン |
| 認証 token | 標準出力にも生成レポートにも転記しない (URL のホスト名のみ表示) |

### 2.2 過去事故との関係

| 事故 | 本スクリプトでの再発防止 |
|------|------------------------|
| 2026-01-06 $195 課金 (Claude が複数回 DB 書き込み) | WRITE 系 SQL を allowlist で完全排除、SELECT のみ許可 |
| 2026-04-03 無料枠浪費 (DROP+CREATE 反復) | 1 回実行で完了する設計、READ 上限 100 で abort |
| 2026-03-18 Turso 未反映 (ローカル更新だけ) | 本検証で同期状態を可視化 → REMOTE_MISSING を検出可能 |

---

## 3. 前提環境

### 3.1 Python 依存

```bash
pip install requests
```

(他に追加依存なし、`sqlite3` は標準ライブラリ)

### 3.2 環境変数 (`.env` から読み込み)

`hellowork-deploy/.env` に以下が設定されていることを確認:

```
TURSO_EXTERNAL_URL=libsql://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io
TURSO_EXTERNAL_TOKEN=<Bearer token>
```

### 3.3 ローカル DB

```
data/hellowork.db
```

URI モード read-only でオープン。書き込みは Python レベルでも禁止。

---

## 4. 実行手順

### 4.1 接続確認 (Turso 接続せず、READ 試算のみ)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts/verify_turso_v2_sync.py --dry-run
```

期待出力:

```
[DRY-RUN] Turso 接続なし
  対象テーブル: 37 件
  READ 試算: 37 (テーブル一覧) + 74 (各テーブル COUNT + sample) + 1 = 約 76
  READ 上限: 100
  ローカル DB: ... (size=N B)
  TURSO_EXTERNAL_URL  : 設定済
  TURSO_EXTERNAL_TOKEN: 設定済
```

### 4.2 本番実行

#### Bash の場合
```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a
source .env
set +a
python scripts/verify_turso_v2_sync.py
```

#### PowerShell の場合
```powershell
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
Get-Content .env | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}
python scripts/verify_turso_v2_sync.py
```

期待所要時間: 3〜10 秒

### 4.3 オプション

| 引数 | 用途 |
|------|------|
| `--dry-run` | Turso 接続せず、READ 試算と環境変数チェックのみ |
| `--db PATH` | ローカル DB パス変更 (default: `data/hellowork.db`) |
| `--output PATH` | レポート出力先指定 (default: `docs/turso_v2_sync_report_YYYY-MM-DD.md`) |
| `--max-reads N` | READ 上限変更 (default: 100) |

---

## 5. レポートの読み方

### 5.1 ステータス一覧

| ステータス | 絵文字 | 意味 |
|-----------|:-----:|------|
| `MATCH` | ✅ | ローカル・リモート完全一致 |
| `COUNT_MISMATCH` | ❌ | 行数が異なる (ローカル更新 or リモート更新が必要) |
| `SAMPLE_MISMATCH` | ⚠️ | 行数は同じだが先頭 5 行のハッシュが異なる (内容が異なる) |
| `LOCAL_MISSING` | 🔴 | ローカルに不在 (リモートのみ存在 → ローカルは古い) |
| `REMOTE_MISSING` | 🟡 | リモートに不在 (ローカルのみ存在 → upload 必要) |
| `BOTH_MISSING` | ⚪ | 両方に不在 (新規投入が必要) |
| `READ_LIMIT` | ⏸️ | READ 上限到達でスキップ |

### 5.2 推奨対応マッピング

| ステータス | 推奨対応 |
|-----------|---------|
| MATCH | アクション不要 |
| COUNT_MISMATCH / SAMPLE_MISMATCH | ローカル → リモートを `scripts/upload_to_turso.py` で再アップロード |
| REMOTE_MISSING | 同上 (initial upload) |
| LOCAL_MISSING | Phase 3 でローカル開発が必要なら `scripts/download_db.sh` で同期 |
| BOTH_MISSING | `SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_INGEST.md` 参照 |
| READ_LIMIT | 翌月 (クォータリセット後) または上限緩和して再実行 |

### 5.3 「追加発見」セクションの意味

レポート末尾の 2 セクション:

- **リモートのみに存在するテーブル** (TARGET_TABLES に未登録): Turso にはあるが本検証スクリプトの想定対象に入っていなかったもの。`v2_flow_*` (Agoop 人流), `v2_salesnow_companies` 等。
- **ローカルのみに存在するテーブル** (TARGET_TABLES に未登録): ローカル開発で生成された分析テーブル等。`layer_a/b/c_*`, `v2_*` 各種集計。

これらは Phase 3 着手前に「対象テーブルを TARGET_TABLES に追加するか」を判断する材料。

---

## 6. 2026-05-04 実行結果の要点

`docs/turso_v2_sync_report_2026-05-03.md` に保存。

### 6.1 サマリ (37 テーブル中)

| ステータス | 件数 |
|-----------|-----:|
| ✅ MATCH | 0 |
| ❌ COUNT_MISMATCH | 1 |
| ⚠️ SAMPLE_MISMATCH | 5 |
| 🔴 LOCAL_MISSING | 29 |
| 🟡 REMOTE_MISSING | 2 |

### 6.2 重要発見 (Phase 3 への影響)

#### 発見 1: ローカル DB が大幅に古い

**29 テーブルが Turso にあるがローカルに不在**。Phase 0 棚卸し (2026-05-03) で「不在 6 テーブル」として識別したテーブルのうち以下は **Turso 側にすでに投入済み**:

| テーブル | ローカル | Turso |
|---------|:-------:|:-----:|
| `v2_external_household_spending` | 不在 | **存在** |
| `v2_external_labor_stats` | 不在 | **存在** |
| `v2_external_establishments` | 不在 | **存在** |
| `v2_external_industry_structure` | 不在 | **存在** |
| `v2_external_land_price` | 不在 | **存在** |
| `v2_external_job_openings_ratio` | 不在 (※) | **存在** |
| `v2_external_turnover` | 不在 | **存在** |
| `v2_external_business_dynamics` | 不在 | **存在** |
| `v2_external_climate` | 不在 | **存在** |
| `v2_external_care_demand` | 不在 | **存在** |

(※) ローカルには `v2_external_job_opening_ratio` (単数形) のみ存在。リモートは `v2_external_job_openings_ratio` (複数形)。**名称ずれ** あり。

→ **Task A の投入手順書の前提が変わる**: 「不在 6 テーブルを新規投入する」のではなく「ローカル DB を Turso と同期する」が正しい対応。Phase 3 で Turso 経由参照のみで設計するなら投入不要 (本番は問題なし、ローカル開発のみ問題)。

#### 発見 2: SalesNow も Turso V2 にすでに存在

`v2_salesnow_companies` が **TARGET_TABLES 未登録** (Plan 設計時の Agent 報告では「別 Turso DB」とされていた) だが、**country-statistics と同じ Turso V2 DB に存在** している。

→ SalesNow 専用 Turso DB (`SALESNOW_TURSO_*`) との二重保有か、設計変更があった可能性。Phase 3 着手前にどちらを参照するか確定する必要。

#### 発見 3: 5 テーブルで内容差分 (SAMPLE_MISMATCH)

| テーブル | 行数 | 内容ハッシュ |
|---------|-----:|:-----------:|
| `v2_external_population` | 1,742 | 不一致 |
| `v2_external_migration` | 1,741 | 不一致 |
| `v2_external_daytime_population` | 1,740 | 不一致 |
| `v2_external_population_pyramid` | 15,660 | 不一致 |
| `v2_external_prefecture_stats` | 47 | 不一致 |

行数は同じだが先頭 5 行の内容が異なる。原因候補:
- ORDER BY 順序の違い (rowid 採番の違い)
- 投入タイミングの違いで値が更新されている
- ヘッダー混入レコードの有無 (本検証は全行取得するので、ヘッダーが入っていれば SAMPLE_MISMATCH になる)

→ Phase 3 着手前に詳細比較推奨 (テーブル単位で `SELECT * WHERE prefecture = '北海道' LIMIT 5` で具体値確認)。

#### 発見 4: REMOTE_MISSING 2 件 (要 upload)

| テーブル | ローカル | Turso |
|---------|:-------:|:-----:|
| `v2_external_minimum_wage` | 存在 | **不在** |
| `v2_external_commute_od` | 存在 (83,402 行) | **不在** |

→ `upload_to_turso.py` で本番反映が必要。特に `commute_flow_summary` の生データになる `commute_od` は Phase 3 でクリティカル。

#### 発見 5: 大量の追加 Turso テーブル

リモートに 15 テーブル追加発見:
- `v2_flow_*` (Agoop 人流データ): 12 テーブル
- `v2_industry_mapping`
- `v2_posting_mesh1km`
- `v2_salesnow_companies`

ローカルには 34 テーブル追加 (`layer_a/b/c_*`, `v2_*` 集計・分析テーブル)。

→ Phase 3 で参照するテーブルが TARGET_TABLES に含まれているか、再棚卸し推奨。

---

## 7. Phase 3 着手前のアクション

| # | アクション | 担当 | 優先度 |
|--:|-----------|------|--------|
| 1 | SAMPLE_MISMATCH 5 件の詳細比較 (どちらが正本か確定) | ユーザー判断 | 高 |
| 2 | REMOTE_MISSING 2 件を `upload_to_turso.py` で Turso 反映 | ユーザー手動実行 | 高 |
| 3 | Task A 手順書の方針更新 (新規投入 → ローカル同期 or Turso 経由のみ参照) | 後続 docs 修正 | 中 |
| 4 | `v2_external_job_opening_ratio` ↔ `v2_external_job_openings_ratio` の名称ずれ解消 | Rust 実装時に確認 | 中 |
| 5 | SalesNow の Turso V2 vs SalesNow 専用 Turso のどちらを使うか確定 | 設計判断 | 高 |
| 6 | Phase 3 着手後、定期的 (月 1 回程度) に再実行して同期保つ | 運用 | 低 |

---

## 8. トラブルシューティング

### 8.1 `WRITE 系 SQL を検出` エラー

スクリプト本体に WRITE 系 SQL が混入した場合の防御。**起こるべきではない**。発生時はスクリプトのバグなので修正が必要。

### 8.2 `READ 上限到達`

TARGET_TABLES を増やしすぎた場合に発生。`--max-reads 200` で上限を上げるか、TARGET_TABLES を絞る。Turso 無料枠 300/月を意識。

### 8.3 `Turso 接続失敗`

- 環境変数の確認: `echo $TURSO_EXTERNAL_URL` (空でないこと)
- token 期限切れ: Turso ダッシュボードで再発行
- ネットワーク: `curl https://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io/health` で疎通確認

### 8.4 `ローカル DB が見つかりません`

- `data/hellowork.db` の存在確認
- `--db <絶対パス>` で別パス指定

### 8.5 文字化け (Windows コンソール)

スクリプトは `sys.stdout.reconfigure(encoding="utf-8")` で UTF-8 化済み。それでも文字化けする場合:

```bash
chcp 65001  # コンソールを UTF-8 に切り替え
```

レポート Markdown 自体は UTF-8 で正しく保存される (コンソール表示のみの問題)。

---

## 9. 完了条件

- [x] スクリプト `scripts/verify_turso_v2_sync.py` 作成
- [x] `--dry-run` で接続テスト成功
- [x] 本番実行で 37 テーブル比較完了 (READ 13 消費)
- [x] レポート `docs/turso_v2_sync_report_2026-05-03.md` 出力
- [x] WRITE 系 SQL 0 件発行
- [x] READ 上限到達なし
- [x] 認証 token がレポートに転記されていない

Task C は完了。Phase 3 着手前のアクション (§7) はユーザー判断。
