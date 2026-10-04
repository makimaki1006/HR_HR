# HR_HR Headless CRM 設計

更新日: 2026-09-28
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

Durable Retry の保存技術は別途実装設計で決める。
この文書は特定 DB / Redis / Turso 等を既決事項にしない。

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

## 12. User Feedback

保存状態を UI で区別する。

- 緑: HubSpot 保存済み
- 黄: 一時保存済み / HubSpot 同期待ち
- 赤: どこにも保存できていない

Queue への保存にも失敗しているのに「保存しました」と表示してはいけない。

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
