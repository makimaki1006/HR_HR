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


---

## ADR-020 — 認証 Cookie の失効を既存ログイン履歴で確認する

Status: Accepted for this implementation (2026-10-11、PR #146 レビュー修正)。履歴の照会頻度・既存メモリへの失効範囲は ADR-021 で変更。

- 現行案: 認証情報だけを暗号化 Cookie に持ち、サーバの固定鍵と有効期限で検証する。再起動をまたげるが、サーバ側の永続的な失効確認は持たない。
- 具体的な問題: ログアウト前に保存した Cookie を再送すると認証が復活する。Cookie の削除だけでは「ログアウト後は拒否」「セキュリティを下げない」を満たせない。
- 採用する変更: Cookie に既存 `login_sessions` の参照を追加し、要求ごとに本人・ログイン方法・成功状態・未終了を照合する。ログアウトと再ログインで元の履歴を終了し、終了済みの Cookie はメモリ状態の有無を問わず拒否する。新しい DB・サービス・表・列は追加しない。アカウント無効化の既存確認、社外パスワード期限、Google OIDC 限定の認可、CSRF は維持する。
- 変更の利点: 保存・更新済み Cookie の再送も、ログアウト後・サーバ再起動後・別インスタンスで拒否できる。ログイン履歴の既存終了日時を利用できる。
- 現行案を維持する利点: DB の照会が少なく、監査 DB の障害中も Cookie だけで本人確認を続けられる。ただしログアウトの個別失効要件を満たせないため採用しない。
- 障害時の扱い: 起動時に監査 DB が未接続なら警告してメモリ認証のみ。履歴の保存が失敗した場合も Cookie は発行しない。発行済み Cookie の鍵がない状態のログアウト、または失効確認が失敗した場合は503で拒否し、Cookie は残して回復後に再試行する。認証 Cookie を使ったログアウトの失効記録が失敗した場合も503で、完了扱いにしない。鍵なしのメモリ認証のログアウトは従来どおり監査失敗時もメモリ状態を削除する。照会結果をキャッシュせず、失効確認のため監査 DB の読み取りが増える。
- 移行・戻すコスト: スキーマ移行なし。参照を持たない旧 Cookie は一度再ログインが必要。同じ固定鍵と同じ既存監査 DB を使えば以後の再起動で復元できる。戻す場合は固定鍵を未設定にして再起動し、メモリ認証へ戻す (再ログインが必要)。既存ログイン履歴はそのまま利用できる。1年保持の既存の掃除で履歴が消えた Cookie は再ログインが必要。


---

## ADR-021 — 認証 Cookie の履歴照会をメモリへの復元時だけにする

Status: Accepted for this implementation (2026-10-11、ユーザーが指定した PR #146 の追加レビュー修正)

- 現行の決定: ADR-020 は Cookie を持つ要求ごとに既存監査 DB で失効確認する。同じ応答の Cookie 更新でも照会していた。
- 具体的な問題: 多数の API を呼ぶ画面に DB の往復・読み取り費用が加わり、監査 DB 障害時には確認済みの利用者まで503になる。2026-01の超過請求事故も踏まえ、追加の読み取りを減らす必要がある。
- 採用する変更: メモリに同じ本人・ログイン方法・履歴参照がある要求は履歴を照会しない。一致しない復元時だけ1回照会し、Cookie 更新時は重ねて照会しない。ログイン成功時の参照は保存済みの履歴から得る。Cookie の暗号・期限・ログイン方法の検証、社外パスワード期限、既存のアカウント無効化確認は維持する。
- ログアウトの扱い: DB への終了日時の保存は維持する。成功したログアウト・再ログインの履歴参照を同じプロセス内に Cookie の最大寿命24時間だけ保持する。容量で追い出さず、期限経過後に掃除する。同じ履歴から複数のメモリ状態が復元されていても、このプロセス内ではすべて拒否する。失効書き込み失敗は従来どおり503とし、失効成功扱いにしない。新しい永続保存先・表・列・サービスは追加しない。
- 変更の利点: 通常要求の追加の履歴照会は0回、復元時は1回。監査 DB 障害中も確認済みのメモリ認証を継続できる。Cookie 単独の再送や再起動後の再送は終了済み履歴で拒否できる。
- 現行を維持する利点と変更の制約: 要求ごとの確認なら、他インスタンスに残る既存メモリにも即時失効が効く。今回の方式では別インスタンスで既に確認済みのメモリへ失効を即時通知せず、メモリ消失・期限切れで次の復元が必要になるまで継続できる。ユーザーが指定した可用性と費用の改善のため、この境界を採用する。既存の無効化判定や画面固有の DB 照会は別に残るため、監査 DB の呼び出し総数が0になるとは限らない。
- 障害時の扱い: 復元が必要な場合の照会失敗は503で Cookie を残す。メモリが一致する場合は追加の履歴照会がないので、その障害による503は返さない。アカウント無効化確認の照会失敗時の既存方針は変えない。
- 移行・戻すコスト: Cookie 形式・鍵・環境変数・スキーマの変更なし。戻す場合は要求ごとの履歴確認を復活させる。プロセス内の失効情報は再起動で消えるが、その後はメモリも消えるので DB の復元時確認で終了済みを拒否する。

---

## ADR-022 — HR_HR はメールの記録を HubSpot に作る

Status: Accepted (2026-10-11 ユーザー決定。依頼の ADR-019 は既存の求人再送 ADR と重なるため、022 を使用（main の ADR-021 も使用済み）)

- 現行の決定: HubSpot が業務履歴の正本 (ADR-005)。Call の作成は Zoom 純正連携に任せる (ADR-007)。
- 具体的な問題: HR_HR の画面から本人の 1 対 1 営業メールを送る必要がある。HubSpot の公開 emails API は記録用で、1 対 1 送信の公開 API はない。マーケティング向け single-send は今回の用途ではない。
- 採用する変更: 操作者本人の Gmail API で送信し、HR_HR が HubSpot の emails に SENT の記録を作り、取引と宛先・CC の関連コンタクトに結び付ける。Call は引き続き作らない。`hubspot_owner_id` は取引所有者ではなく操作者本人を使用する。
- 追加許可: 既存 Google Workspace OIDC の client に incremental authorization と offline consent を追加する。gmail.send / gmail.readonly を同時に要求する (readonly は送信結果確認と今後の返信取り込み用)。state・PKCE・ID token の nonce / aud / 署名 / ドメイン / 本人一致を確認する。
- 保存先: 既存 audit Turso の `crm_gmail_grants` に refresh token を AES-256-GCM (32 バイトの専用環境変数鍵、操作者 email を AAD) で暗号化して保存する。新しい DB は作らない。取り消し時はローカルの token を消し、Google へも取り消しを要求する。Google の取り消しが失敗した場合は利用者へ Google アカウントでも取り消すよう案内する。
- 冪等・再試行: 送信前に既存 `crm_pending_operations` にメール操作を永続化する。Gmail の送信結果と HubSpot の記録結果を分ける。worker はメールを送らず、記録のみ再試行する。送信途中の中断は Gmail の RFC Message-ID を検索して確認する。HubSpot は CREATE 前に `hs_email_message_id` で検索し、見つかれば作成しない。CREATE の結果が不明な場合は検索の反映を待ち、見つからなくても二度目の CREATE は行わない。上限後は管理者確認が必要になる。
- 返信取り込み: 本人の追加許可で gmail.readonly を使い、既定 10 分おき（`CRM_EMAIL_REPLY_INTERVAL_SECS`）に HR_HR の送信台帳にある送信後 30 日以内のスレッドだけを自動確認する。自分・許可ドメインからの返信は除外し、自動返信を含む外部の受信を同じ取引・コンタクトの HubSpot emails に記録する。
- 返信の保存と負荷: 既存 audit Turso に返信 ID・条件付き処理確保の `crm_gmail_reply_receipts` と確認時刻・許可状態の `crm_gmail_reply_status` を加える。本文は既存保存待ちで記録完了までだけ保持し、HubSpot の Message-ID 検索と永続的な処理確保で二重起動を防ぐ。新着がなければ HubSpot を呼ばず、新着は巡回あたり最大 50 件、記録と再試行は共有 gateway の Background 優先度を使う。
- 保持: メールの操作番号は七日を過ぎても再送を許さない。メール操作を既存 PATCH の七日掃除から除外し、記録済み・Gmail に拒否された操作は本文・件名・宛先を除いた小さな重複防止の受領記録にする。未反映の業務情報は記録が終わるまでのみ保持する。HubSpot の業務履歴を複製する表ではない。
- 変更の利点: 本人の Gmail から送れる。HubSpot に履歴と操作者が残り、失敗時にメールを重ねて送らない。既存の監査ストアと再試行 worker を利用できる。
- 現行を保つ利点: Gmail の許可・鍵管理・記録の再試行が増えない。ただし HR_HR からの送信はできず、Gmail と HubSpot の手作業になる。
- 移行・戻すコスト: 表 1 つと既存台帳 payload の任意フィールド。`CRM_EMAIL_ENABLED` を未設定にすれば停止できる。既存ログインと CRM PATCH は維持する。戻す場合は Gmail の許可を取り消し、既に送信済みのメールと HubSpot の記録を残す。
- 制約: `CRM_EMAIL_ENABLED=1`、専用鍵、追加 callback URL、既存 Google 設定、監査接続がそろった場合だけ有効。トークン・本文をログへ出さない。現行の単一 Render インスタンスで操作ごとのロックを共有する。返信取り込みには audit 台帳の条件付き claim を使う。送信 API を複数インスタンスで動かすには送信側にも条件付き claim が別途必要。

参照: [Google OAuth (offline / incremental)](https://developers.google.com/identity/protocols/oauth2/web-server)、[Gmail send](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/send)、[HubSpot email activity](https://developers.hubspot.com/docs/api-reference/legacy/crm/activities/emails/guide)。

## ADR-023 — HR_HR 送信メールの計測情報は既存監査 Turso に置く

Status: Accepted for this implementation (2026-10-11、ユーザー指定)

- 現行の決定: HubSpot が Contact / Deal / Email 活動の正本 (ADR-005)。独自 CRM DB は作らない (ADR-006)。
- 具体的な問題: HubSpot の標準メール開封・クリック数は API 書き込み不可。Gmail で送信したメールに対する HR_HR の計測には、公開リンクの転送先と再起動後も読める集計が必要。
- 採用する変更: 既存監査 Turso に `crm_email_tracking`・`crm_email_tracking_links`・`crm_email_tracking_events` を追加。本文・連絡先の複製は持たず、操作者、参照 ID、件名、送信 ID、転送先、計測時刻と自動確認の可能性だけを保存する。生 IP・User-Agent・Referer は保存しない。CRM_EMAIL_ENABLED=1 と暗号化鍵がある場合のみ、既存の起動時初期化で表を作る。実 DB の設定・起動はユーザーが行う。
- 変える利点: 新規 DB / サービスを増やさず、HR_HR 固有の計測を永続化する。メールの業務履歴と関連付けは引き続き HubSpot に置く。
- 現行を保つ利点: DB 負荷が増えない。ただし独自計測は再起動で失われ、転送リンクも維持できない。
- 制約: 同一計測 ID・種類・リンク・自動判定の組合せはプロセス内で60秒に1回、DBでは1分の窓で1回にまとめる。保存は全体120件/分、DB照会は公開ルート全体600回/分、同時DB処理8件、キャッシュ各2000件まで。上限時の計測は失われる。件数は実際の人の操作回数の保証ではない。Apple の画像プロキシなど識別情報のない自動取得は判別できない。
- 保持期間: 既存 activity と同じ1年 (365日)。送信時刻 (`sent_at`、未送信の準備は `created_at`) から365日より古い送信記録・リンク・出来事を、既存の監査 purge (起動10分後、その後24時間ごと) で削除する。親記録の削除に連動するSQLiteトリガーにより、3表を同じトランザクションで削除し、失敗時はまとめて戻す。機能OFF・鍵未設定でも既存表の削除を続け、未導入なら表を作らず何もしない。削除済みの転送リンクは404になり、プロセスの既存キャッシュも最大60秒で消える。件名・転送先は認証済みCRMユーザーだけに返す。
- 移行 / 戻すコスト: 表3つとBの計測モジュール・公開2ルート・集計1ルート・表示部品。環境変数を未設定にすれば計測と表示を止められる。ただし既送信メールの転送リンクも404となるため、停止は既送信リンクへの影響を考慮する。

### 送信 PR との接続

- `&AuditDb` をそのまま渡す (同じDBアダプター・同時処理上限を使用)。明示する場合は `AuditTrackingStore::new(audit.clone())` を共有し、共通ブリーフの `instrument_html` / `bind_sent` を呼ぶ。送信処理・鍵の検証・再送台帳はAが担当。
- HubSpot保存待ちなら `bind_sent(..., None, gmail_message_id)`。保存完了後、同じ計測ID・Gmail IDで `Some(hubspot_email_id)` を結び付け直せる。既存の別IDへの変更は拒否。
- 公開側の有効化条件は `CRM_EMAIL_ENABLED=1`・`CRM_EMAIL_TOKEN_KEY` が標準Base64の32バイト・監査DBあり。鍵はAと同じ標準Base64の32バイトを検証し、無効な鍵では表初期化・計測・表示を無効にする。
- HubSpot保存失敗時に本文を再度 instrument したり Gmail を再送したりしない。最初の計測IDをAの再送状態に保持する。

- 結合時の本文 (ユーザー指定で判断変更): 従来は相手に届けた計測付き HTML を HubSpot に残していた。これは本文の完全一致を保てる利点があるが、HubSpot のタイムライン・プレビュー・通知から社員が閲覧すると相手の開封・クリックとして数えてしまい、HR_HR の表示側対策では防げない。HubSpot の `hs_email_html` は計測前の本文に変更し、Gmail にだけ計測付き本文を渡す。社内の HubSpot 閲覧による自己計測を防ぐ利点を優先する。移行・戻すコストは既存の再送台帳 payload に任意の計測前本文を保持する変更のみで、新しい保存先は増やさない。本文は記録完了・送信拒否時に両方とも消し、以前の送信 PR の台帳は追加フィールドなしでも読める。CRM の表示時の画像除去・元リンクへの復元も、過去の本文への対策として維持する。送った本人が Gmail の送信済みメールを開いた場合も、開封として数えることがあります。
- 計測障害: 送信前の計測保存に失敗した場合は、計測前の本文で送る。保存されていない転送先は本文に含めない。送信後の ID 結合失敗も送信・HubSpot 記録の成功を失敗や再試行へ変更しない (その場合、計測表示は欠落しうる)。送信の重複を防ぐ操作台帳の保存は引き続き必須で、台帳保存に失敗した場合は送らない。
