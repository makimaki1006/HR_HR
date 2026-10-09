# CRM 架電画面の負荷試験 (ローカル、本物の HubSpot には繋がない)

100 人が同時に架電画面 (`/app/crm`) を使ったとき、Rust 側の `/api/crm/*` がどう振る舞うかを手元で測る。
HubSpot と Google は `fake_hubspot.mjs` (偽物) に置き換える。依存パッケージは無い (Node 18 以上と python3)。

| ファイル | 役割 |
|---|---|
| `fake_hubspot.mjs` | 偽 HubSpot (アプリが架電画面で使う API: search / batch read / objects / 関連 v3・v4 / owners / pipelines / properties / property groups / 関連ラベル / access-token-info) と偽 Google OIDC (`/oidc/*`)。遅延・レート制限・429・呼び出し回数の記録 |
| `run.mjs` | 偽 HubSpot とアプリを起動し、仮想ユーザーをログインさせてシナリオを流し、レポートを書く |
| `before-main-base-url.patch` | main (2026-10-08 時点) には HubSpot の接続先を変える環境変数が無いため、計測時だけ当てたローカル変更。**コミットしない** |
| `results/` | 実行結果 (`<日時>-<label>.md` / `.json` / `.app.log`)。git には入れない |

## 使い方

```bash
npm ci                                                    # ルート (この試験自体は npm パッケージを使わない)
python3 scripts/e2e/make_fixture_db.py /tmp/fixture.db    # アプリの起動に要る最小 DB
# main で測る場合だけ: 接続先の上書きを一時的に当てる (gateway ブランチは不要の見込み。下記)
git apply scripts/loadtest/before-main-base-url.patch
CARGO_TARGET_DIR=$PWD/target-private cargo build --bin rust_dashboard   # debug ビルド (下記)
node scripts/loadtest/run.mjs --bin target-private/debug/rust_dashboard --db /tmp/fixture.db --label before
git apply -R scripts/loadtest/before-main-base-url.patch  # 戻す
```

主なオプション (既定値):

| オプション | 既定 | 意味 |
|---|---|---|
| `--users` / `--admins` | 100 / 10 | 仮想ユーザー数 / うち管理者 (`ADMIN_EMAILS`。キューの既定が全員分) |
| `--duration` | 300 | シナリオの長さ (秒) |
| `--rush-seconds` | 10 | 朝の一斉起動: 全員がこの秒数の中で画面を開く |
| `--think-min` / `--think-max` | 30 / 90 | 操作の間隔 (秒) |
| `--client-timeout-ms` | 35000 | 画面 (React の `apiGet`) と同じ打ち切り |
| `--latency-ms` / `--jitter-ms` | 400 / 150 | 偽 HubSpot の応答時間 (一様分布で ±jitter) |
| `--limit-10s` / `--limit-1s` | 190 / 19 | 一般 API の上限 (直近 10 秒 / 直近 1 秒) |
| `--search-per-sec` | 5 | Search の上限 (一般 API とは別枠。`--search-counts-general true` で一般枠にも数える) |
| `--background-rps` | 0 | 外部バッチの模擬: 一般枠を毎秒この数だけ消費する |
| `--deals` | 20000 | 合成する Deal の数 |
| `--app-port` / `--fake-port` | 9418 / 9400 | |
| `--spawn false` | | 起動済みのアプリ・偽 HubSpot を使う (環境変数は `run.mjs` の `startProcesses` と同じにすること) |
| `--report <file.json>` | | 保存した実行結果 (`results/*.json` と同名の `.app.log`) からレポートだけ作り直す |
| `--seed` | 42 | シナリオの乱数 (同じ値なら同じ操作列。ただし応答時間で順序は揺れる) |

## 何をしているか

### 接続先

- **HubSpot**: アプリを `HUBSPOT_ACCESS_TOKEN=fake-loadtest-token`・`HUBSPOT_BASE_URL=http://127.0.0.1:9400` で起動する。
  main の `src/main.rs` は `DEFAULT_BASE_URL` (api.hubapi.com) 固定なので、計測時だけ `before-main-base-url.patch`
  (`HUBSPOT_BASE_URL` を読む 3 行) を当てた。gateway ブランチ (feat/hubspot-gateway) で接続先の環境変数名が違う場合は
  `run.mjs` の `HUBSPOT_BASE_URL` を合わせる。
- **ログイン**: `/api/crm/*` は Google ログイン (OIDC) の人しか通さない (`src/crm/rbac.rs`)。PR の E2E は
  パスワードログイン + 架空サンプル (`?mode=fixture`) なので `/api/crm/*` を呼ばない。そこで、ローカルの OIDC E2E
  (`tests/e2e/oidc_local/`) と同じ **debug ビルド限定** の `GOOGLE_OIDC_DISCOVERY_URL_DEBUG` で偽 Google に向け、
  テスト鍵 (`tests/fixtures/oidc/test_key_1.pem`) で署名した ID token で 100 人分のセッションを作る
  (`lt-user000@f-a-c.co.jp` 〜)。本番の認証は変えていない (release ビルドにはこの分岐が入らない)。
  このため計測は debug ビルドで行う (アプリ自体の CPU は HubSpot の待ち時間に比べて小さいが、release より遅い点に注意)。
- 監査 DB (Turso) は繋がない。役割は `ADMIN_EMAILS` だけで決まる (先頭 `--admins` 人が管理者)。

### シナリオ (1 人あたり)

1. **朝の一斉起動**: 0〜`rush-seconds` 秒のどこかで画面を開く = `/api/nav`・`/api/crm/metadata`・`/api/crm/property-catalog`・
   `/api/crm/call-queue/pipelines`・`/api/crm/owners`・`/api/crm/call-queue?limit=50` を並列に呼ぶ (画面のマウント時と同じ)。
   条件は 80% が既定のパイプライン (bpo_リクロジ)、20% が `default`。30% がステージを絞る。
   管理者以外の 30% は `owner=all`、管理者の 30% は `owner=me` (残りは既定 = 管理者は全員分・それ以外は自分)。
   2〜6 秒後に 1 件目の案件 (`/api/crm/workspace/deals/{id}?deal_props=..` 既定の項目付き) を開く。30% は続けて 2 ページ目を読む。
2. **定常**: 30〜90 秒ごとに次の操作を 1 つ。
   - 10%: 条件を変えてキューの 1 ページ目を読み直す
   - 直前の案件を 60 秒以内に開き直す (半分は `fresh=1`)
   - それ以外は次の案件を開く。20% は「人気の案件」(誰かが最初に読んだキューの先頭 5 件。複数人が同じ案件を開く)。
     読み込んだ行の終わりが近ければ先に次のページを読む (スクロール)
   - 案件を開いたあと 25% は 10〜60 秒後に同じ案件を `fresh=1` で読み直す (通話後の自動読み直し)

書き込み (架電結果の保存) はまだ API が無いので含めない。

### レポート

`results/<日時>-<label>.md` に以下を書く (生データは同名の `.json`、アプリのログは `.app.log`)。

- 朝の一斉起動 (各人の最初の画面表示 + 最初の案件) と定常に分けた、エンドポイントごとの p50 / p95 / p99 / 最大と、200 以外の内訳 (`error_kind` 付き)
- 各人がキューを見られるまでの時間 (5 / 10 / 20 / 35 秒以内の人数)
- 偽 HubSpot 側: 呼び出し種類ごとの回数と 429、毎秒の平均・最大、直近 10 秒の最大 (外部バッチ込みも)、Search/秒、同時処理数の最大
- 10 秒ごとの推移と、アプリのログの retry / timeout の件数

### 偽 HubSpot の作り

- Deal 20,000 件 (70% が `753186575`、30% が `default`。ステージは `frontend/src/generated/call_queue_pipelines.json` の表から、
  未済に寄せる)。次回架電日・最終架電日・架電禁止理由などは乱数 (固定シード) で埋める。
  Contact (Deal ごとに 1〜2 人)、Company (8,000 社を共有)、通話 0〜14 件 (+ 担当者だけに付く通話 0〜4 件)、メモ・メール・ミーティング。
- Search は `filterGroups` (OR) / `filters` (AND) の EQ・IN・HAS_PROPERTY・NOT_HAS_PROPERTY・GT/GTE/LT/LTE・BETWEEN、`sorts`、`after`、`query` を実際に評価する
  (1 万件の窓を超えると 400)。Search の応答には `x-hubspot-ratelimit-*` を付けない (本物と同じ)。
- レート制限は到着時に判定する。拒否は 429 + `Retry-After` (秒) + `x-hubspot-ratelimit-*`。拒否は枠を消費しない。
- 遅延は成功応答にだけ掛ける (429 は 40 ms)。
- 状態確認: `GET /_stats` (種類別の回数と毎秒の推移)、`POST /_reset`。

## 測定上の注意

- 1 回の実行で判断しない。偽 HubSpot の遅延は乱数なので、比較は同じオプションで複数回回して範囲で見る。
- 本物の HubSpot の応答時間・上限の挙動 (どの時点で数えるか、Retry-After の有無) とは一致しない。ここで見られるのは
  「アプリが HubSpot をどう叩くか (回数・並び・待ち)」と「その結果利用者に何が返るか」。
- 同じマシンでアプリ・偽 HubSpot・ドライバを動かす。CPU が詰まると数字が歪むので、実行中は他の重い処理を止める。

## 書き込みシナリオ (`--scenario write`)

架電結果の保存 (`PATCH /api/crm/deals/{id}`) を 100 人が同時に行ったときの失敗率・台帳 (`crm_pending_operations`) の滞留・復旧時間を測る。
読み取りのシナリオとは別のドライバ (`write_run.mjs`) を `run.mjs` が呼ぶ。結果と注意点は `docs/architecture/crm-write-loadtest-2026-10-09.md`。

```bash
# Node 23.11 以上 (偽 Turso が node:sqlite を使う)。debug ビルド必須 (偽 Google・再送待ちの上書きは debug だけ)
CARGO_TARGET_DIR=$PWD/target-private cargo build --bin rust_dashboard
CARGO_TARGET_DIR=$PWD/target-private node scripts/loadtest/run.mjs --scenario write --variant v1   # v1 | v2 | v3 | v4
```

| 変種 | 内容 |
|---|---|
| `v1` | 偽 HubSpot は正常 (遅延 450 ± 150 ms、一般枠 190/10 秒・19/秒) |
| `v2` | PATCH の 5% を 503 にする + t=120〜150 秒に全面障害 (70% は 503、15% は書いた上で 503、15% は書いた上で 10 秒超ハング) |
| `v3` | t=120〜140 秒、全リクエストを 429 (Retry-After 2) にする |
| `v4` | 全員が同じ 10 秒のうちに 10 項目を保存 → その後 60 秒は通常の動き |
| `v5` | 20 人が同じ 5 案件の同じ項目を同じ base で同時に保存 (期待: 案件ごとに 1 件成功 + 残り 409。上書きで消えた更新を数える) |

1 人 3 件の案件を持ち (衝突しない)、5〜15 項目を 0.15〜0.5 秒間隔で 1 項目ずつ PATCH → 25% でステージ移動 → 10〜40 秒の間 → 繰り返し。
429 / 503 / タイムアウトは同じ `operation_id` で最大 2 回やり直す (`--retries`)。負荷が終わったあと台帳が空になるまで最大 `--drain-max` 秒(既定 300) 見続ける。
主なオプション: `--users` `--duration` `--backoff-secs` (再送待ち。debug の上書きは一定値) `--turso-latency-ms` `--latency-ms` `--app-env K=V,..` (例: 関所 `HUBSPOT_APP_RATE_PER_SEC=19,HUBSPOT_APP_RATE_PER_10S=190`) `--write-rate` (`CRM_WRITE_RATE_PER_MIN` を読むバイナリのときだけ効く)。

偽 HubSpot に `POST /_chaos` ({storm429, outage, patch503Rate, retryAfter, hangMs})、偽 Turso (`tests/e2e/crm_write_live/fake_turso.mjs`) に `--latency-ms` と `GET /_stats` (文=動詞+表の回数) を足した。
