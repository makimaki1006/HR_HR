# HR_HR Headless CRM 設計

更新日: 2026-10-08
状態: 設計方針確定 / 実装前

## 1. 背景

現在、テレアポ BPO のアルバイトに HubSpot Core Seat を割り当てている。
Core Seat は 1人 12,000円/月だが、BPO は月数回程度しか利用しないケースがあり費用対効果が低い。

目的は HubSpot を置き換えることではない。

**HubSpot を System of Record / CRM Engine として維持し、
人が日常的に操作する UI を HR_HR 側へ移す。**

## 2. 基本アーキテクチャ

```text
Browser
│
│ React + TypeScript
│ HR_HR CRM Workspace
│
▼
Rust / Axum (HR_HR / Render)
│
├ HubSpot Adapter
├ Zoom Phone Adapter
├ Authorization / RBAC
├ Validation
├ Retry / Pending Sync
└ System Logging
     │
     ├──────────────► HubSpot
     │                System of Record
     │
     └──────────────► Zoom Phone
                      Telephony
```

HR_HR 本体と同じ Rust / Axum / Render アプリケーションへ載せる。
別サービスへ分離することを前提にしない。

Frontend は HR_HR の React 移行方針に従う。

## 3. System of Record

CRM の正本は HubSpot。

HubSpot に保持する。

- Contact
- Company
- Deal
- Owner
- Property
- Pipeline / Stage
- Call Activity
- 架電結果
- メモ
- 次回架電日時
- Task
- Workflow trigger 用 Property
- 業務 Activity

独自 CRM DB は作らない。

ユーザー数が増えること自体を DB 導入理由にしない。

## 4. Rust / HR_HR の責務

Rust は CRM データの Master にはならない。

責務:

- API endpoint
- HubSpot API access
- Zoom Phone integration
- Authentication
- RBAC
- Input validation
- HubSpot error translation
- Retry
- Idempotency
- System log
- HubSpot deep link generation

## 5. Frontend

新規 CRM Workspace は React + TypeScript + Vite で作る。

主な Client State:

- selected Company
- selected Contact
- selected Deal
- call queue
- Zoom Phone state
- current call
- zoom_call_log_id
- call duration
- input memo
- call result
- next call date/time
- HubSpot sync state
- retry state

Zoom Phone の UI / state は CRM Workspace 内で常駐させる。

想定 UI:

```text
┌──────────────┬──────────────────────────┬──────────────┐
│ Call Queue   │ Contact / Company / Deal │ Zoom Phone   │
│              │ Activity                 │              │
│              │ HubSpot Deep Link        │ Smart Embed  │
├──────────────┴──────────────────────────┤              │
│ Call Result / Memo / Next Call          │              │
│                          Save & Next     │              │
└─────────────────────────────────────────┴──────────────┘
```

## 6. HubSpot Deep Link

Rust / Frontend で HubSpot object ID を利用する。

- Contact
- Company
- Deal

各画面に「HubSpotで開く」を表示する。

URL そのものを正本として保存するのではなく object ID を基準に生成する。

## 7. Zoom Phone

Zoom Phone は既存契約を継続利用する。

HR_HR から発信できる UI を提供する。

Call Activity は Zoom Phone for HubSpot (純正連携) が作る。HR_HR は Call を作らない
(ADR-007、2026-09-29 改訂)。

HR_HR が行うこと:

- Smart Embed のイベントから callId / callLogId、相手番号、発信時刻を受け取る
- HubSpot 上の Call を相手番号 (`hs_call_to_number` の正規化) と時刻窓で照合する
- 見つかった Call と同じ Contact / Deal に、架電結果の Note と Property 更新を紐づける
- 見つからなければ Pending Sync で遅延再照合し、一定時間後も無ければ管理画面に出す
- API での Call 作成は、純正連携が記録しなかった場合の手動フォールバックに限る

読み取り時の注意 (実データで確認済みの罠):

- Call は Deal と多対多。1 件に潰さない
- 接触は Deal より Contact に付くことが多い。Deal の直近アクティビティは Deal → Contact → Call も辿る
- Call の系統は `hs_call_source` で区別する (`INTEGRATIONS_PLATFORM` = 純正連携)
- Deal の contact 経由の通話には、同じ Contact の**別 Deal** の通話も混ざりうる (絞り込みは PR4 以降)
- 読み取りは 1 リクエストあたり HubSpot 呼び出し最大 6 回 (本体+関連 1 / Deal の contact→calls 1 / Engagement 型ごと batch read 4)、安全装置で 10 回まで、全体 20 秒で 504 `crm_timeout`。鍵を既存バッチと共有しているため
- emails は v1 では読まない (共有鍵に email 読み取りスコープがあるか未確認。無いとレコード全体が 403 になりうる)
- 架電画面の詳細 (`GET /api/crm/workspace/deals/{id}`) は順番に待つ段が 3 つ (案件 + 関連 ID → 担当者・会社・メモ等 → 通話)。担当者・会社の関連ラベルは関連ラベルの定義 (6 時間キャッシュ) で付ける (2026-10-08)
- 同じ詳細はサーバで 60 秒キャッシュし、利用者をまたいで共有する (最大 500 件)。認可 (rbac とレコード単位の関門) はキャッシュがあっても毎回通す。欠けのある応答は入れない。`?fresh=1` (画面の「最新にする」と、発信した通話が終わった後の自動の読み直し) でその案件の分を捨てて読み直す。HubSpot へ書き込む操作を足すときは、その案件のキャッシュを捨てる (`WorkspaceCache::invalidate_deal`) (2026-10-08 ユーザー承認)

純正連携の全機能を HR_HR で再現しない。

## 8. HubSpot Workflow

既存 HubSpot Workflow は原則維持する。

Rust 側に Workflow business logic を全面コピーしない。

基本:

```text
BPO input
↓
Rust
↓
HubSpot Property update
↓
Existing HubSpot Workflow
↓
Stage / Task / Notification / Owner etc.
```

Call Activity は履歴表示用。
Workflow の業務トリガーは Property を中心にする。

API で作成した Call Activity が、Zoom 純正連携由来の event trigger と完全同等に動くとは仮定しない。

## 9. Logging

### Zoom Phone 通話ログ
Zoom Phone が一次情報。
必要な業務情報は HubSpot Call Activity へ反映する。

### HubSpot CRM Activity
顧客との業務履歴の正本。

### Rust System Log
Render Logs。
例:
- HubSpot API success / failure
- Zoom event received
- timeout
- retry
- validation error

CRM Activity と system log を混同しない。

### Audit
通常の業務監査は HubSpot Activity / Property History へ返す。
Service Key 経由では実操作者が HubSpot native source だけで分からない場合があるため、
operator identity を Activity へ明示的に含める。

厳密な append-only compliance audit が将来必要になった場合のみ別途再検討する。

## 10. Durable Retry / Pending Sync Queue

目的:
HubSpot へデータが返らない外的障害時に、BPO が入力したデータを失わないこと。

これは CRM DB ではない。

保持するのは **HubSpot へまだ反映されていない操作** のみ。

例:

```text
operation_id
operator_id / operator_email
hubspot_contact_id
hubspot_company_id
hubspot_deal_id
zoom_call_log_id
operation
payload
status
retry_count
last_error
created_at
next_retry_at
```

正常時:

```text
Frontend
↓
Rust
↓
HubSpot
↓
200 OK
↓
完了
```

一時障害:

- 429
- 5xx
- timeout
- transient network failure

```text
HubSpot write fail
↓
Pending Sync Queue
↓
backoff
↓
retry
↓
HubSpot recovery
↓
success
↓
Queue から削除
```

恒久エラー:

- 400 validation
- 401 authentication
- 403 permission

無限 retry しない。

```text
FAILED / Dead Letter
↓
管理者通知
↓
内容確認
```

Durable Retry の保存技術は ADR-018 (2026-10-09) で **既存の監査 Turso の表 `crm_pending_operations`** と決めた。

### 実装状況 (2026-10-09、`src/crm/write.rs` / `pending.rs`)

- 実装済み: `PATCH /api/crm/deals/{id}` (既定 OFF。`CRM_WRITES_ENABLED` / `CRM_WRITE_DEAL_ALLOWLIST`)、
  一時障害 (429・5xx・タイムアウト・接続失敗・混雑) だけを `pending` にして 202、worker が再送
  (1 分・5 分・30 分・2 時間・以降 6 時間、最大 8 回。**再送のたびに競合と必須項目を確かめ、HubSpot の値が変わっていたら上書きせず `failed`**)。
- 恒久エラー (400 / 401 / 403 / 404 / 422)・競合・上限超えは即 `failed`。管理者の一覧 `GET /api/admin/crm-operations?status=failed|pending`、
  再試行 / 破棄 `POST /api/admin/crm-operations/{id}/retry|discard`。
- 表は起動時に自動作成 (`CREATE TABLE IF NOT EXISTS`)。Turso への書き込みは受付・送信結果ごと・1 日 1 回の掃除だけで、
  worker は書き込みでポーリングしない (期限が来た行の SELECT のみ。再送待ちが無ければ 10 分おき)。
- 上限 `CRM_PENDING_MAX` (既定 50000) を超える一時障害は 503 `queue_full` (台帳は `failed`、再送しない)。
- 台帳に記録できない (監査 DB 未接続・Turso 障害) ときは HubSpot に書かず 503 `queue_unavailable`。
- 未実装: Dead Letter の管理者通知 (一覧に出るだけ)、再送成功後のキャッシュ即時破棄 (60 秒 / 30 秒で自然に切れる)、
  複数インスタンスでの worker の排他 (現状 Render は 1 インスタンス前提)。

## 11. Idempotency

Retry を安全にする。

特に CREATE 系:

- Call Activity
- Task
- Note

は二重作成対策必須。

利用候補:

- operation_id
- zoom_call_log_id

例:

```text
zoom_call_log_id = ABC123
↓
HubSpotに既にあるか確認
├ YES → CREATEしない
└ NO  → CREATE
```

Property PATCH のような同一値更新は比較的 retry しやすい。

実装 (2026-10-09): PATCH はクライアントが付ける `operation_id` を台帳 (`crm_pending_operations`) に**送る前**に記録し、
同じ `operation_id` の再送は保存された結果をそのまま返す (HubSpot へは 2 度書かない)。PATCH 自体も同じ値なら冪等。
再送では「HubSpot の現在値が書く値と同じ」なら保存済みとして扱う (応答だけ失われた書き込みを競合にしない)。
Call / Task / Note の CREATE 系は ADR-007 により HR_HR では作らないので、この仕組みの対象外。

## 12. User Feedback

保存状態を UI で区別する。

- 緑: HubSpot 保存済み
- 黄: 一時保存済み / HubSpot 同期待ち
- 赤: どこにも保存できていない

Queue への保存にも失敗しているのに「保存しました」と表示してはいけない。

実装 (2026-10-09): 緑 = 200 `saved`、黄 = 202 `queued` (`GET /api/crm/operations/{id}` で `pending` / `retrying` / `saved` / `failed` を確かめる)、
赤 = 409 競合・422 検証/必須不足・503 `queue_full` / `queue_unavailable`・ネットワーク失敗。

## 13. RBAC

Core Seat を外すことで、HubSpot native write permission を BPO の安全境界として使えなくなる。

Rust Backend が新しい権限境界になる。

BPO の例:

許可:
- Contact / Company read
- Call create
- 指定 Property write
- 指定 Task create

禁止例:
- Delete
- Deal amount update
- unrestricted owner change
- unrestricted pipeline change
- 他部署 record への不要アクセス

HubSpot Service credential を Browser に渡さない。

## 14. Backup / Recovery

Headless CRM 専用の CRM backup DB は作らない。

CRM data は HubSpot が正本。
Zoom 通話一次情報は Zoom。

ただし HubSpot の保持 / restore 機能は完全な無期限 snapshot ではないため、
会社として必要な retention policy は Headless CRM とは別軸で定義する。

## 15. DB を追加する条件

以下のときのみ再検討する。

**HubSpot にも Zoom にも置くべきではない、HR_HR 固有の永続データが発生した場合。**

例:
- 独自 business entity
- HubSpot と独立した長期 state
- compliance 上必要な別監査情報

以下だけでは DB 導入理由にしない。

- ユーザー数が増えた
- API 呼び出しが増えた
- 一般的な Web App では DB を置くことが多い

## 16. Pricing / Reversibility

この設計は「永久に Core Seat を回避できる」ことを前提にしない。

HubSpot の価格 / API / permission model が変わり経済合理性を失った場合、
Core Seat 運用へ戻すことは可能。

したがって最大の要件は vendor pricing の完全予測ではなく、
今の期間に削減できる費用と業務 UX が実装 / 保守費を上回ること。

## 17. 未決事項

実装前に確定する。

- HubSpot scopes (書き込みで付けるスコープと、書ける Property の許可リスト。PR4 前)
- Zoom Phone Smart Embed の認証・イベント設計
- Pending Sync Queue の具体的な durable storage (候補: 既存 audit Turso への表追加 / 既存 Google Sheets 連携。AGENTS.md rule 7)
- 書き込みの retry/backoff policy (読み取りは下記で決定済み)
- Dead Letter の管理者通知先
- HubSpot custom properties の追加有無
- 役割 (RBAC role) の種類と保持先 (ADR-017 の候補から選ぶ)。決まるまで `/api/crm/*` は Google ログインかつ role=admin だけ許可
- レコード単位のアクセス範囲 (他部署のレコードを読めるか)

決定済み (2026-09-29、ユーザー決定):

- HubSpot の認証: 既存の Service Key (sales-automation-api) を共有して使う。環境変数は `HUBSPOT_ACCESS_TOKEN`。HR_HR 専用キーは発行しない
- 読み取りの retry/backoff: 既存バッチとレート上限を共有するため、Search は HR_HR から 1 req/秒まで。429 は max(Retry-After, 最低 1 秒) を待って最大 2 回、401/403 は retry しない
- 純正ログの再現: しない。純正連携が作った Call に紐づける (ADR-007 改訂、§7)
- BPO も会社の Google Workspace アカウントを持つ。CRM の認可は Google ログインを前提にする
- React App Shell と共通部品は platform-team が作り、CRM 画面はそれを使う (`/app/crm`)
- Rust API の型は ts-rs で TypeScript に生成する (React Phase 0 と同じ手段)
- CRM 画面は HTMX を挟まず React で作る (ADR-014 補足)

## 18. HubSpot 呼び出しの関所 (2026-10-08)

状態: 実装済み (PR「HubSpot 呼び出しの関所」)。CRM 利用者 約 100 人の同時利用に備える。

### 背景 / 問題

- 鍵は既存の外部バッチと共有している。HubSpot の上限 (応答ヘッダで確認: 10 秒 190 回・1 秒 19 回・1 日 625,000 回、Search はアカウントで約 5 回/秒) をアプリと外部バッチで分け合う。
- これまで流量の制限は Search の 1 回/秒 (クライアントごと) だけだった。CRM 用・営業KPI 用・求人票コピー用でクライアントが別々にあり (求人票コピーは独自の HTTP クライアント)、アプリ全体の上限が無かった。
- 429 を受けても止まるのはその 1 本だけで、他の要求はそのまま送り続けていた。
- 同じ案件を同時に開くと、同じ読み取りを人数分送っていた。
- 架電キューは毎回 HubSpot を読んでいた (案件の詳細だけ 60 秒キャッシュがあった)。

### 決定

1. **関所 (`src/hubspot/gateway.rs`)**: プロセスに 1 つ。本番の HubSpot クライアントは全部 `HubSpotClient::for_production` でこれに繋ぐ (CRM・求人票コピー・営業KPI 直読み・管理画面の鍵確認)。求人票コピーの独自クライアントは `HubSpotClient::read_raw` (retry しない生の読み取り) に置き換えた。画像連携の運用 CLI の書き込み 2 本 (プロパティ作成・PATCH) は読み取り専用クライアントに書き込み API を置かない方針のため独自の HTTP のまま、ただし送る前に関所の許可を得る。
   - 窓: Search 以外は「1 秒 8 回・10 秒 80 回」、Search は「1 秒 3 回」(環境変数で変更可。`docs/env_variables_reference.md` §2d-2)。直近の許可時刻で数えるので、どの区間を切り取っても超えない。HubSpot の上限の残り (10 秒 110 回・1 秒 11 回・Search 2 回/秒) は外部バッチの分。Search は従来クライアントごとに 1 回/秒だったが、負荷試験 (100 人、2026-10-08) で朝の架電キューの先頭ページの約 9 割が 20 秒の締め切りに間に合わず、HubSpot の上限には届いていなかったため 3 回/秒に上げた (§17 の「Search は 1 req/秒」の決定をこの節で置き換える)。
   - 優先度: `Interactive` (画面の操作) と `Background` (定義系キャッシュの先読み・営業KPI 直読み・運用 CLI)。待ち行列は優先度ごとで、画面の操作の列が空のときだけ背景が進む。
   - 待ちの上限: 画面の操作 5 秒・背景 60 秒。並んだ時点の見込み (窓が空くまで + 前に並ぶ数 × 間隔) が上限を超えるなら並ばずに、並んだ後でも上限に達したら列から抜けて、HubSpot を呼ばずに `HubSpotError::Busy` → 503 `hubspot_busy`。画面は「HubSpot が混み合っています。少し待ってから再試行してください」。締め切りで打ち切られた要求の順番待ちは列から外れる (後ろの人の Search の順番を食わない)。
   - 同時実行の枠: 架電キュー (8) と案件・レコードの読み取り (8) は別の枠にする (キューが Search の順番を待つ間に詳細の読み取りが後ろに並ばないように)。枠の待ちも同じ上限で `hubspot_busy` にする (20 秒待たせて `crm_timeout` にしない)。
   - 429: どの要求が受けても `Retry-After` (無い・0 なら 1 秒、最大 10 秒) の間、関所を通る全員を止める。各要求の retry の決まり (最大 2 回・最低 1 秒) は変えない (retry も 1 回として窓に数える)。
   - 相乗り: 同じ読み取り (優先度・メソッド・パス・query・本文が同じ) が同時に走っていれば 1 回にまとめる。優先度を鍵に入れるのは、画面の操作が背景の取得に相乗りして背景の列で待たされないようにするため。
   - 観測: 応答ごとに `X-HubSpot-RateLimit-*` を記録し、起動してからの回数 (種類別・Search・429・相乗り・断った回数)、待ち行列の長さ、直近 5 分の待ち時間 p50 / p95 を持つ。`GET /api/admin/hubspot-usage` (管理者だけ。数字だけで鍵や中身は返さない) と管理画面 `/app/admin?view=hubspot` (15 秒ごとに更新) で見る。
2. **架電キューのキャッシュ (`src/crm/queue_cache.rs`)**: 1 ページの中身を 30 秒、先頭ページの段階の件数を 60 秒。鍵は「パイプライン・絞り込み・並び・**解決済みの担当者** (`me` は本人の owner ID に直す)・今日 (JST)・ページの位置」。中身は利用者によらない部分だけで、cursor (本人に束縛した署名) と `scope` (役割・所属チーム) は要求ごとに作り直す。認可 (`rbac::authorize`) はキャッシュを見る前に毎回行う。関連の読み取りに欠けがある応答・数えるべき総数を数えられなかった応答は入れない。`?fresh=1` (画面の「再試行」「最初から読み直す」) は読み直して置き換える。上限 500 件 / 1,000 件。
3. **定義系キャッシュの先読み**: パイプライン定義 (5 分)・担当者一覧 (10 分)・プロパティ一覧 (6 時間)・関連ラベル (6 時間) は、有効期間の残りが 20% を切ったら背景の優先度で読み直す (画面の要求は待たない。同時に 1 本)。期限切れ後の最初の要求は従来どおりその場で読む。本人の owner ID (10 分) と CRM の定義 (60 秒、`?refresh`) は従来どおり。

### 現行を保つ利点 / 変える利点 (AGENTS.md rule 16)

- 現行を保つ利点: 待ち行列が無いので応答は速い (上限に当たるまでは)。仕組みが少ない。
- 変える利点: アプリ全体の流量が外部バッチの取り分を食わない。混雑時は 20 秒待たせずにすぐ「混み合っています」を返す。429 の後に全員で叩き続けない。同時に同じ案件・同じキューを開く人数に比例して HubSpot を呼ばない。
- 戻すコスト: 環境変数で上限を上げれば実質的に制限を外せる (`HUBSPOT_APP_RATE_PER_SEC=19` 等)。キャッシュは `fresh=1` で迂回できる。

### 前提・未確認

- HubSpot の Search が 10 秒・1 秒の窓に数えられるかは公開仕様で確定できなかった。関所では Search を別の窓 (1 回/秒) で数え、通常の窓には数えていない。
- 関所はプロセス内だけ。Render でインスタンスを 2 つ以上にすると上限はインスタンス数倍になる (今は 1 インスタンス)。
- 求人票コピーの読み取りは従来どおり retry 1 回 (Retry-After が 2 秒以下のときだけ) を呼び出し側で行う。
