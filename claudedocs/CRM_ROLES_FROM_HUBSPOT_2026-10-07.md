# 架電 CRM の範囲の決め方を変更 (2026-10-07)

> **更新 (同日)**: 「管理者以外は自分の担当分だけ」は `CRM_OWNER_PICK_FOR_ALL_2026-10-07.md` で置き換えた (全員が全件を見られ、所有者を選べる)。この文書の 1・2・3 の範囲の規則は古い。

`CRM_ROLES_IMPLEMENTATION_2026-10-06.md` の役割 (admin / consultant / bpo / user) の規則を、この文書の規則に置き換える。
決定 (ユーザー 2026-10-07): 「会社の Google アカウントでログインした人は全員 CRM のユーザー。管理者以外は自分の担当分だけ見えれば良い」。

## 1. 規則

1. **使えるか**: Google OIDC で本人確認済みで、メールが会社ドメイン (`ALLOWED_DOMAINS`) の人は全員。パスワードログイン・社外ドメインは不可。
   `CRM_METADATA_ALLOWED_EMAILS` は設定されていれば追加条件 (空・未設定なら絞り込まない)。無効化されたアカウントは不可。
2. **管理者**: `ADMIN_EMAILS` に載っている人、または `accounts.role = admin` の人。管理者だけ全件を読める。
3. **管理者以外の全員**: 自分の担当分だけ (BPO かどうか・所属チームは見ない)。
   - キュー: 自分が HubSpot の担当 (owner) の取引でキュー条件に合うものだけ。他人・全員・担当なしの指定は 403 `forbidden_owner`。
   - 個別 Deal: 自分が担当でキュー条件に合うものだけ。Contact / Company: その Deal に紐づくものだけ。外れたら 403 `forbidden_record` (本文は読まない)。
   - 自分の HubSpot owner がメール一致で見つからない人: 403 `owner_not_found`。Owners の取得が失敗したときは HubSpot のエラー。どちらも全件に倒さない。
4. `accounts.role` は **admin かどうかの判定にだけ**使う。consultant / bpo / user の値は判定に使わない (管理画面には「一般」と「admin」だけ出し、古い consultant / bpo は「いまは使わない」と表示)。DB のスキーマ変更・書き込みの追加なし。
5. **HubSpot の所属チーム**は範囲の判定には使わない。架電キューの隅に「自分の担当分だけを表示しています。HubSpot の所属チーム: ○○」と参考表示するだけ (`scope.teams`)。`CRM_BPO_TEAM_NAMES` は導入していない。

## 2. 許可表

| 操作 | 管理者 | 管理者以外 (owner あり) | owner なし |
|---|---|---|---|
| metadata | 可 | 可 | 可 |
| キュー(既定) | 全員分 | 自分 | 403 `owner_not_found` |
| キュー `owner=all` / `unassigned` / 他人の id | 可 | 403 `forbidden_owner` | 403 |
| キュー `owner=me` | 可 | 可 | 403 `owner_not_found` |
| 担当者一覧 `/api/crm/owners` | 可 | 403 `forbidden` | 403 |
| 個別 Deal | 全部 | 自分が担当でキュー条件に合うもの | 403 `owner_not_found` |
| Contact / Company | 全部 | 上の Deal に紐づくもの | 403 |

## 3. 実装の場所

- `src/crm/rbac.rs`: `finalize_role` が admin か否かだけを返す (非管理者は内部値 `CrmRole::Bpo` = 「自分の分だけ」。名前は旧来のもので BPO かどうかは見ない)。`authorize` にドメイン確認を追加。
- `src/hubspot/client.rs`: `owner_by_email` が owner の id と `teams[].name` を返す。
- `src/crm/call_queue.rs`: owner のキャッシュ (10 分) に所属チームを持たせた。`scope.role` は `admin` / `own`、`scope.teams` を追加。
- `src/crm/record_gate.rs` / `workspace.rs` / `routes.rs`: 変更なし (範囲が「管理者以外は自分の分だけ」になったので、従来の BPO の関門がそのまま全員に効く)。
- 画面: 架電キューの文言変更と所属チーム表示 (`CallQueueScreen.tsx`)、管理画面の役割変更を admin / 一般だけに縮小 (`AdminScreen.tsx`)。

## 4. キャッシュ

- owner と所属チームは 10 分キャッシュ (見つからなかった結果は 1 分)。HubSpot でチームを変えた場合、画面の参考表示への反映は最大 10 分。範囲は変わらない (チームを見ていないため)。担当 (owner) の付け替えは取引側の `hubspot_owner_id` の変更で、キューには次の読み取りで反映される。
- `accounts.role` (admin 判定) は 5 分キャッシュ (従来どおり)。

## 5. ユーザーが行うこと

- スキーマ変更・SQL 実行は不要。Render に `CRM_BPO_TEAM_NAMES` を足す必要もない。
- `CRM_METADATA_ALLOWED_EMAILS` は Render で削除済み (設定されていれば追加条件になる)。
- 管理者を増やす / 外すのは `/app/admin` の役割変更 (admin / 一般) か `ADMIN_EMAILS`。
- 全員の HubSpot メールが owner のメールと一致している必要がある (一致しない人は `owner_not_found` で何も見えない)。
