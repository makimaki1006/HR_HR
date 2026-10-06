# 架電 CRM の役割 (RBAC) 本実装 (2026-10-06)

決定 (ユーザー 2026-10-01): 役割は admin / consultant / bpo / user の 4 つ。保存先は audit Turso の `accounts.role`(列は既存、スキーマ変更なし)。BPO は架電キューに出たレコードだけ読める。
関連: `src/crm/rbac.rs`(判定・キャッシュ)、`src/crm/record_gate.rs`(BPO のレコード制限)、`src/handlers/admin/json.rs`(役割の変更 API)、`frontend/src/screens/admin/AdminScreen.tsx`(変更 UI)。

## 1. 役割の判定規則

| 状況 | 結果 |
|---|---|
| `accounts.role` が admin / consultant / bpo / user | その役割(前後の空白・大文字小文字は吸収) |
| 未知の値・空・全角・`admin,bpo` など | user(CRM 不可。最小権限) |
| 同じメールの行が複数 | 最小権限 |
| 行なし | user。ただし `ADMIN_EMAILS` の人は admin(ログイン時に admin で自動登録されるのと同じ扱い) |
| 監査 DB 未接続・照会失敗 | `ADMIN_EMAILS` の人だけ admin(非常口)、他は user。**キャッシュの古い値は使わない** |

- 5 分キャッシュ(メールごと)。失敗は 30 秒だけ覚える。管理画面で役割を変えたら同じプロセスのキャッシュを即時に捨てる(別プロセスは最大 5 分)。Render が 1 インスタンスなら即時。
- `ADMIN_EMAILS` は「accounts を読めないとき・行がないときの非常口」として残した。行があれば行が正。ただし `ADMIN_EMAILS` の人はログインのたびに admin へ戻る(既存の昇格処理)ため、管理画面での降格は 409 `env_admin` で断る。
- `CRM_METADATA_ALLOWED_EMAILS` は「追加の絞り込み」に変更。**設定されているときだけ**そのリストにも入っている必要がある(AND)。**空・未設定なら役割だけで決まる**。従来は「空 = 全員拒否」だった。BPO を管理画面だけで増やすには、Render の同変数を空にする(`render.yaml` は `s_fujimaki@f-a-c.co.jp` を宣言している)。

## 2. 許可表

| 操作 | admin | consultant | bpo | user |
|---|---|---|---|---|
| metadata | 可 | 可 | 可 | 不可 |
| キュー(既定) | 全員分 | 全員分 | 自分 | 不可 |
| キュー `owner=all` / `unassigned` / 他人の id | 可 | 可 | 403 `forbidden_owner` | 不可 |
| キュー `owner=me` | 可 | 可 | 可 | 不可 |
| 担当者一覧 `/api/crm/owners` | 可 | 不可 | 不可 | 不可 |
| 個別 Deal | 全部 | 全部 | 自分が担当でキューの条件に合うもののみ | 不可 |
| 個別 Contact / Company | 全部 | 全部 | キューの条件に合う自分の Deal に紐づくもののみ | 不可 |

- 不可 = 403 `forbidden`。HubSpot は 1 回も呼ばない(テストで呼び出しログ 0 件を確認)。
- BPO が外れたレコード = 403 `forbidden_record`。本文は返さない。存在しない id も同じ 403(404 で存在を教えない)。owner が HubSpot に無い BPO は 403 `owner_not_found`。

## 3. BPO のレコード単位の制限(方式)

本文を読む前に関門(`record_gate::bpo_may_read`)を通す。
- Deal: 1 回読み、`call_queue::deal_in_queue` で判定。条件 = パイプライン 753186575、`hubspot_owner_id` が本人の owner、アーカイブでない、`bpo_3`・`bpo_4` が空、ステージが未済(常に)または許可ステージで `bpo_13` が今日(JST)以前。
- Contact / Company: `→ deals` の関連を 1 回引き、関連 Deal(先頭 100 件)を batch で 1 回読んで、1 件でも上の条件に合えば可。
- 本人の owner は既存のキャッシュ(10 分)を使う。HubSpot 障害で関門が読めないときは通さない(上流の本文も返さない)。
- **電話番号の有無は見ない**(Contact / Company の追加読み取りが要るため)。電話番号が無い自分の担当 Deal は、キューには出ないが個別取得はできる。キューと完全一致させるなら、関門に電話番号の判定(+2〜3 回)を足す。

## 4. 役割の変更

`POST /api/admin/users/{account_id}/role` `{"role":"bpo"}`(`/app/admin?view=user&id=...` の「架電 CRM の役割」)。管理者(`accounts.role = admin`)だけ。
- 400 `invalid_role` / 403 `cannot_change_self`(自分は変えられない) / 404 `account_not_found` / 409 `env_admin` / 502 `audit_write_failed`。
- 変更すると operation log に `change_role`(変更前後・メール)を残す。書き込みは `accounts.role` の 1 列(アプリ実行時に管理者の操作で)。

## 5. ユーザーが行うこと

1. スキーマ変更・SQL の実行は不要(`accounts.role` は既存の `TEXT NOT NULL DEFAULT 'user'`)。
2. Render の `CRM_METADATA_ALLOWED_EMAILS` を空にするか削除する(BPO・consultant を役割だけで増やす場合)。残すなら、その一覧に入っていない人は役割があっても拒否される。
3. 本番反映後、自分(`ADMIN_EMAILS` = admin)で `/app/admin?view=users` から各人の役割を設定する。初期状態の他の人は user(CRM 不可)。
   緊急時に SQL で直す場合の例(実行はユーザー): `UPDATE accounts SET role = 'bpo' WHERE lower(email) = lower('xxx@f-a-c.co.jp');`(アプリのキャッシュは最大 5 分で追従)。
4. BPO の HubSpot owner 対応: BPO のメールが HubSpot の owner のメールと一致している必要がある(Owners API の完全一致)。一致しない人は `owner_not_found` で何も読めない。
