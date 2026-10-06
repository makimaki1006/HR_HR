# 営業KPI の HubSpot 直読みを本番で有効にする手順書

作成: 2026-10-06 / 対象: PR #66(`SALES_KPI_HUBSPOT_DIRECT=1`、既定オフ)

## 前提と守ること

- 鍵の env 名は `HUBSPOT_ACCESS_TOKEN`(`src/config.rs`)。営業KPI の直読み(`hubspot_direct.rs`)、架電 CRM(`src/crm/`)、求人票作成(`job_copy_live.rs`)は**同じ env の同じ鍵**を読む。別の鍵は無い。
- 鍵の値は誰にも見せない。診断 API も値・一部・ハッシュを返さず、ログにも出さない。
- **AI は Render の env 値を保存・上書きしない**(既存値が空に見える事故があったため)。env の追加・変更・削除はすべてユーザーが Render のダッシュボードで行う。
- 必要な scope(#66 の推測。診断 API が実際の有無を返す): `crm.objects.deals.read` / `crm.objects.owners.read` / `crm.schemas.deals.read`。

## 手順

| # | 内容 | 誰が |
|---|---|---|
| 1 | この PR のデプロイ後、管理者アカウントでログインしたブラウザで `https://hr-hw.onrender.com/api/admin/hubspot-check` を開く。`/health` の commit が PR のマージ後であることも確認 | ユーザー |
| 2 | 結果を読む(下の表)。結果の JSON(鍵は含まれない)を AI に貼れば判定を手伝える | ユーザー / AI(読み取りのみ) |
| 3 | scope が足りない・鍵が無効なら下の「足りないとき」へ | ユーザー |
| 4 | `all_required_present: true` を確認したら、Render の Environment に `SALES_KPI_HUBSPOT_DIRECT` = `1` を追加して保存(再デプロイが走る) | ユーザー(AI は触らない) |
| 5 | 有効化直後の確認(下) | ユーザー / AI(ログ・値の突き合わせの手伝い) |
| 6 | 問題があれば戻す(下) | ユーザー |

### 1-2. 診断 API の読み方

`GET /api/admin/hubspot-check`(管理者専用。一般ユーザーは 403、未ログインはブラウザなら `/login` へ、`Accept: application/json` の API 呼び出しなら 401)。HubSpot への呼び出しは 1 回だけ、30 秒で打ち切り、成功した結果は 5 分キャッシュ(`cached: true` で分かる)。

| 見る所 | 意味 |
|---|---|
| `configured: false`(HTTP 503、`error_kind: not_configured`) | Render に `HUBSPOT_ACCESS_TOKEN` が入っていない(または空)。入れるのはユーザー |
| `error_kind: hubspot_auth`(502) | HubSpot が鍵を受け付けない。鍵が失効・無効、または作り直し後に Render の env が古い |
| `error_kind: hubspot_upstream` / `not_found`(502) | トークン情報 API がこの種類の鍵に対応していない可能性(下の「代替確認」)。5xx なら HubSpot 側の障害 |
| `error_kind: hubspot_timeout` / `hubspot_transport` | HubSpot に届かない・遅い。少し待って再実行 |
| `all_required_present: true` | 3 つの scope が揃っている。手順 4 へ |
| `all_required_present: false` | `required` の `present: false` の scope が足りない。「足りないとき」へ |
| `portal_id` | 鍵が属するポータル。`23708633`(HUBSPOT_PORTAL_ID 既定)と一致するか確認 |
| `scopes` | 鍵が持つ scope の全部(参考) |

### 足りないとき(ユーザー)

1. HubSpot の設定 → 連携 → プライベートアプリ(または鍵の種類に応じた画面)で、該当の鍵の「スコープ」に不足分を追加する。読み取り(`.read`)だけでよい。書き込み scope は足さない。
2. 鍵を**作り直した場合**(ローテーション)は、新しい値を Render の `HUBSPOT_ACCESS_TOKEN` にユーザーが貼り替えて保存する。古い鍵を使う他の仕組み(GitHub Secret `HUBSPOT_TOKEN` の日次バッチなど)も同時に更新が要るか確認する。
3. 診断 API を開き直す。キャッシュは 5 分、env 更新で再デプロイされればキャッシュも消える。

### 代替確認(トークン情報 API が使えない鍵のとき)

トークン情報 API(`POST /oauth/v2/private-apps/get/access-token-info`)は private app のアクセストークン向け。**【未確認】** Service Key など他の種類の鍵で使えるかは本番で一度試すまで分からない(本 PR は本物の HubSpot を呼んでいない)。`hubspot_upstream` / `not_found` が出た場合の代わりの確かめ方は、`SALES_KPI_HUBSPOT_DIRECT=1` を有効にして Render ログの `営業KPI: HubSpot 直読みの更新に失敗しました` の `error_kind`(`hubspot_auth` ならトークン・scope の問題)を見る方法、または架電 CRM の `/api/crm/owners`(owners.read)を管理者で開いて 200 になるかを見る方法。足りない scope の特定には HubSpot 側の鍵の設定画面を見る(ユーザー)。

## 有効化直後の確認(手順 5)

1. **バナー**(ユーザー): 営業KPI 画面(`/app/sales-kpi`)の上部に「HubSpot から直接取得: HH:MM 時点(5 分ごとに更新)」が出る。デプロイ直後の数十秒は「取得中」になり、自動で読み直す。
   - 「最新の取得に失敗しました(種別、時刻)。… 時点の値を表示しています」→ stale。失敗種別を控える。黙ってシートには戻らない。
   - 値が一度も取れず 503 → Render ログを見る(下)。
2. **Render ログ**(ユーザー。AI はログを貼ってもらえば読める): 成功は `営業KPI: HubSpot 直読みを更新しました`(`requests`・`took_secs` 付き)、失敗は `営業KPI: HubSpot 直読みの更新に失敗しました(直前の値を出し続けます)`(`error_kind` 付き)。更新は 5 分ごと。鍵未設定のときの案内は `SALES_KPI_HUBSPOT_DIRECT=1 ですが HUBSPOT_ACCESS_TOKEN が未設定(または不正)です`。
3. **旧シートとの値の突き合わせ**(ユーザー。AI が集計の手伝い可): 直読みは HubSpot 由来の 6 ブロック(商談・アポ・Cヨミ・決定者の当日分・メンバー・架電リスト全社)だけを置き換え、残り(担当別・リスト在庫・架電日次・週次・決定者の過去日・Zoom 項目)はシートのまま。
   - 有効化の前に、同じ画面の主要な数字(今月の商談数・アポ数・担当別の件数など)を控えておく(スクリーンショット)。
   - 有効化後、同じ判定日・同じ範囲で見比べる。シートは日次バッチ更新、直読みは 5 分更新なので、**同じ時刻でなければ差が出る**。差が出たら「HubSpot で直接数えた値」を正として、差の件数と該当の担当者・商談を控える(推測で「ずれていない」と言わない)。
   - 退職者・集計除外(シート「KPI営業_集計除外」)の扱いが差の原因になりやすい。
4. 数分後に再度バナーの時刻が進んでいること(5 分ごとの更新が回っていること)を確認する。

## 戻し方(手順 6、ユーザー)

Render の Environment から `SALES_KPI_HUBSPOT_DIRECT` を削除(または空に)して保存・再デプロイ。画面は従来のシート読みに戻る。`HUBSPOT_ACCESS_TOKEN` は架電 CRM と求人票作成も使うので**消さない**。

## 補足

- 診断 API は読み取りのみで、HubSpot のデータは読まない(鍵の scope とポータル ID の問い合わせだけ)。
- 診断の結果を見ただけでは、HubSpot のレート制限の残りや実データの取得成否までは分からない。最終確認は手順 5 の実画面とログ。
