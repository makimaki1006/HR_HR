# Architecture Decision Records — Frontend / Headless CRM

更新日: 2026-09-29

この文書は既決事項の短い索引。
詳細は以下を参照。

- `docs/architecture/frontend-react-migration.md`
- `docs/architecture/headless-crm-design.md`
- `docs/architecture/react-full-migration-plan.md`(全画面移行の波・完了条件・撤去手順)

---

## ADR-001 — Rust / Axum を維持する

Status: Accepted

Backend の Rust / Axum は置き換えない。

理由:
- Business Logic
- Data processing
- Security
- API integration
- compile-time safety

を既に担っており、現在の問題の中心は Backend ではなく Frontend の state / lifecycle 複雑化である。

---

## ADR-002 — Frontend 標準を React + TypeScript + Vite へ段階移行する

Status: Accepted

HTMX 採用当初は合理的だったが、HR_HR が Data / Operations Application 化し Client State が増えているため。

全面一括 replacement はしない。

### 追記(2026-09-29): 「全画面」の範囲

ユーザー指示(2026-09-29): 「React に移行するのは、いずれは全部」「フロントエンドの React 化は決定事項で、全てにおいて実行する」。
これにより、段階移行の終点を「全画面を React に移し、HTMX 依存と旧テンプレートを撤去する」と定める(本 ADR の本文は段階移行と一括置換の禁止だけを決めていた)。

- 印刷・ダウンロード用のレポートも React に移す(`/app/print/*` の印刷用画面。SSR は使わない、ADR-015)。印刷品質(A4、改ページ、印刷時の再描画)を落とさない。
- UI から外した非表示タブ(市場概況・地域カルテ・詳細分析・総合診断・トレンド・都道府県比較・条件診断・求人検索)、dead route(overview / demographics / balance / workstyle)、proposal-mock、架電クオリティの未実装タブも React に移す。削除はせず、ナビ定義 `/api/nav` の `hidden` フラグで隠す(いつ復活させるか分からないため)。
- login / logout だけは Rust 側のサーバ HTML に残す(OIDC のコールバック・Cookie・レート制限が Rust にあるため)。admin / my は React に移す。
- いま追加実装中のもの(媒体分析の競合調査、Headless CRM、コンサルKPI の UI/UX 改修)は、それぞれの作業が落ち着いてから扱う。Headless CRM は最初から React(ADR-003)。

移す順番(W1〜W10)、完了条件、撤去手順は `react-full-migration-plan.md` を正とする。

---

## ADR-003 — Headless CRM は React で新規実装する

Status: Accepted

理由:
- Zoom Phone 常駐 state
- selected Contact / Company / Deal
- input draft
- sync state
- retry state

など Client State が明確に存在する。

---

## ADR-004 — HR_HR と Headless CRM は同じ Rust / Axum / Render Application に載せる

Status: Accepted

別 Render Service を前提としない。

Frontend technology は既存画面と CRM で段階的に異なってよい。

---

## ADR-005 — HubSpot を System of Record とする

Status: Accepted

CRM master を独自 DB に複製しない。

Contact / Company / Deal / Property / Activity / Workflow は HubSpot を正とする。

---

## ADR-006 — 独自 CRM DB を作らない

Status: Accepted

DB 追加条件は「HubSpot / Zoom に置くべきではない HR_HR 固有の永続 data が発生した場合」。

ユーザー数増加だけでは追加しない。

---

## ADR-007 — Zoom Phone 通話一次情報を HubSpot Call Activity へ反映する

Status: Accepted (2026-09-29 改訂: Call は API で作らず、純正連携が作った Call に紐づける)

~~現行 HubSpot × Zoom Phone の実利用項目を棚卸しし、必要な項目だけ再現する。~~ (改訂前の文言)

Call Activity の作成は Zoom Phone for HubSpot (純正連携) に任せる。
Headless CRM は純正連携が作った Call を探して、架電結果の Note / Property 更新をその Call と同じ Contact / Deal に紐づける。
API での Call 作成は、純正連携が記録しなかった場合の手動フォールバックに限る。

改訂の記録 (AGENTS.md rule 16):

- 現行の決定: Zoom Phone の通話一次情報を HR_HR から HubSpot Call Activity へ反映する (実利用項目を棚卸しして再現)。
- 具体的な問題: 反映は既に純正連携が行っている (2026-04-15 時点の直近 90 日サンプル 500 件で `hs_call_source=INTEGRATIONS_PLATFORM`。BPO のキュー経由架電も対象)。HR_HR からも作ると同じ通話が二重に登録され、止めるには純正連携の設定変更が要る (BPO だけ除外できるかは未確認)。
- 変更内容: Call は作らない。Smart Embed のイベントで得た callId / 相手番号 / 時刻から、HubSpot 上の Call を番号 + 時刻窓で照合して紐づける。見つからなければ Pending Sync で遅延再照合し、一定時間後も無ければ管理画面に出す。
- 変える利点: 二重登録が構造的に起きない。純正連携の設定を触らない。`hs_call_duration` の単位や録音 URL 形式を再現する作業が要らない。
- 現行を保つ利点: HR_HR が Call の中身を完全に制御でき、純正連携の障害 (2026-08-31〜09-01 の Deal 関連付け停止のような事象) の影響を受けない。
- 移行 / 戻しのコスト: 照合ロジック (番号の正規化 + 時刻窓) の実装が増える。戻す場合は Call 作成 API を足し、純正連携の対象から BPO を外す作業が要る。
- 決定: 2026-09-29 ユーザー決定 (計画補足 P-1)。未検証の前提: Smart Embed から発信した通話が純正連携で HubSpot に記録されること (同じ Zoom Phone ユーザーの通話なので記録される見込みだが、実機で 1 件確認してから PR6 に着手する)。

---

## ADR-008 — HubSpot Workflow を残す

Status: Accepted

Rust に Workflow business logic を全面複製しない。

Property update → existing Workflow を基本とする。

---

## ADR-009 — Durable Retry / Pending Sync を持つ

Status: Accepted

HubSpot の一時障害時に、人が入力したデータを失わない。

ただし Queue は CRM copy ではない。
未反映 operation のみ保持する。

具体 storage は未決定。

---

## ADR-010 — CREATE 系は Idempotency を必須とする

Status: Accepted

Call Activity / Task / Note 等の二重作成を防ぐ。

operation_id / zoom_call_log_id 等を利用する。

---

## ADR-011 — Rust Backend を RBAC の境界とする

Status: Accepted

HubSpot Service credential は Browser に渡さない。

BPO が書ける object / property / operation を Rust 側で制限する。

---

## ADR-012 — HubSpot Deep Link を UI に提供する

Status: Accepted

Contact / Company / Deal の object ID から「HubSpotで開く」を提供する。

---

## ADR-013 — System Log と CRM Activity を分離する

Status: Accepted

- CRM業務履歴 → HubSpot
- Zoom通話一次情報 → Zoom / HubSpotへ反映
- Rust system error / retry → Render Logs

---

## ADR-014 — React 移行は小さい既存画面で先に検証する

Status: Accepted

Headless CRMを React 導入可否の実験台にしない。

React + Rust API + TypeScript contract + CI + Playwright の経路を小さな既存画面で先に成立させる。

### 補足(2026-09-29): Headless CRM は Phase 1A の完了を待たない

ユーザー判断(2026-09-29): 「HubSpot の能力をアプリ側に持たせる件は、HTMX で作ると React 移行時の実装コストが高いので、最初から React で作ってよい」。

- Phase 0(PR #28、main にマージ済み ad7d918)で、Vite のビルド、`/app/{screen}` の配信、ts-rs の型生成、CI の frontend ジョブまでの経路は成立した。PR ごとの Playwright E2E はまだ無く、Phase 1A(1A-5)で整備中。
- Headless CRM の React 画面は、Phase 1A(採用診断)と並行して Phase 0 の基盤の上で作ってよい。
- 「CRM を React 導入可否の実験台にしない」という本 ADR の趣旨は変えない。導入可否は Phase 0 で判断済みとして扱う。
- PR ごとの E2E の共通ジョブが整う前に CRM 画面を公開する場合は、CRM 側で E2E を持つ。

---

## ADR-015 — Next.js を現時点では導入しない

Status: Accepted

Rust / Axum が Backend / Server responsibility を持つため。
React + TypeScript + Vite を採用する。

---

## ADR-016 — HubSpot pricing 永続性を設計前提にしない

Status: Accepted

API / Seat 条件が将来変わって採算が悪化した場合、Core Seat 運用へ戻せる。

Vendor pricing を予測し切ることより、設計の可逆性を維持する。

---

## ADR-017 — 社内ユーザーのログインを Google Workspace OIDC にする

Status: Accepted

社員・BPO アルバイトとも会社発行の Google Workspace アカウント (f-a-c.co.jp) を持つ。
Headless CRM の RBAC と操作者記録 (headless-crm-design §9, §13) には
本人確認済みの identity が必要で、現行の「許可ドメインのメール + 共有パスワード」では担保できない。

- ID token の検証は Rust 側で行う (署名・aud・`hd` = f-a-c.co.jp・`email_verified`)
- Client secret は Browser に渡さない
- 社外向けの期限付きパスワード (AUTH_PASSWORDS_EXTRA / ALLOWED_DOMAINS_EXTRA) は当面残す。廃止は別途決定
- 役割 (admin / consultant / BPO 等) の保持先は未決定。候補:
  - 既存 audit Turso の accounts.role (列は既存)
  - Google グループ (Workspace 管理側での設定が必要)
  - 環境変数 (ADMIN_EMAILS と同じ方式。変更に再デプロイが要る)

---

## ADR-018 — Headless CRM の書き込みの再送待ち・操作台帳は既存の監査 Turso に置く

Status: Accepted (2026-10-09 ユーザー決定。書き込み機能は既定 OFF で出荷)

ADR-009 が保留していた「Durable Retry / Pending Sync の保存技術」を決める。

- 現行の決定: ADR-009 は「Queue は CRM copy ではない。未反映 operation のみ保持する。具体 storage は未決定」。headless-crm-design §17 の候補は「既存 audit Turso への表追加 / 既存 Google Sheets 連携」。
- 具体的な問題: BPO が入力した値を HubSpot の一時障害で失わず、二重に書かず、誰が何を変えたかを残す必要がある。プロセス内メモリだけでは再起動 (Render のデプロイ) で消える。
- 変更内容: 監査 Turso (`AUDIT_TURSO_URL`) に表 `crm_pending_operations` を追加する (`src/audit/schema.rs` の `CREATE TABLE IF NOT EXISTS`、起動時に自動作成)。持つのは書き込み操作 1 件ごとの operation_id・操作者・案件 ID・書く値と競合検査用の base・状態・試行回数・次回時刻・結果だけ。HubSpot のレコードのコピーは持たない (ADR-006 の趣旨を維持)。保存済み・失敗の行は 7 日で掃除し、長期の「誰が何を変えたか」は `activity_logs` (1 年保持) に `crm_write` として残す。
- 変える利点: 新しいインフラ (Redis 等) を足さない。操作者の識別 (accounts) と同じ DB で、既にバックアップ・接続管理がある。再起動をまたいで残る。
- 現行 (代替案) を保つ利点: Google Sheets は編集履歴が見えるが、排他・条件付き更新・冪等キーの一意制約が無く、API の割り当てもある。専用 Redis / 別 DB は運用物が増え、ADR-006 の「DB を増やさない」に反する。
- 移行 / 戻しのコスト: 表 1 つ。戻す (別ストレージへ移す) ときは `src/crm/pending.rs` の関数を差し替えるだけで、ハンドラ・worker は台帳の関数だけを呼ぶ。書き込み機能自体は `CRM_WRITES_ENABLED` を空にすれば即座に止まる。
- 制約 / 注意: Turso は過去に請求超過 (2026-01) があるため書き込み回数を絞る。書き込むのは (1) 受付の INSERT (2) 送信結果ごとの UPDATE (3) 1 日 1 回の DELETE だけで、worker は書き込みでポーリングしない。再送待ちの件数上限 `CRM_PENDING_MAX` (既定 50000) を超えたら 503 `queue_full` で積まない。監査 DB が使えないときは HubSpot に書かない (監査できない書き込みは許さない)。Render が複数インスタンスになる場合は worker の排他 (行の取得時の更新) が要る。

関連する決定 (同日、ユーザー):

- 競合は上書きしない (409 と現在値を返す)。再送でも HubSpot の値が変わっていたら `failed`。
- 書けるのは「プロパティ」パネルで選べる全ての項目のうち HubSpot が API で書かせるもの (読み取り専用・計算・`hs_*` / `hubspot_*`・「※編集不可」・`bpo_hsurl` / `bpo_transactio_id`・`dealstage` / `pipeline` を除く)。ステージはパイプライン内・パイプライン間の移動を許す。
- ステージの必須項目は HubSpot が API の書き込みでは強制しないため、HR_HR が移動前に確かめる (`src/crm/stage_rules.json`、HubSpot の画面の内部 API を 2026-10-09 に読み取り専用で写した。6 時間おきに公開のパイプライン API と突き合わせ、ずれを `GET /api/admin/crm-stage-rules-drift` に出す)。
- 失敗 (恒久エラー) した操作は管理者の一覧 (`/api/admin/crm-operations`) に出す。


## ADR-019 — 求人の案の再送は既存監査DBの専用表に保持する

Status: Accepted for this implementation (2026-10-11)

案と履歴の正本はHubSpot。未反映の保存操作・競合確認・重複防止だけを、既存監査Tursoの `job_copy_draft_operations` に保持する。新しいDBは作らず、CRMの取引向けworkerは変更しない。採用理由・代替案・戻すコスト・運用制約は [`jobgen-to-job-copy.md`](jobgen-to-job-copy.md) を参照。
