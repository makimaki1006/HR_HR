# 架電キュー(読むだけ)設計 レーンC 段階1・2

作成: 2026-10-04。状態: **設計のみ。キューの条件がユーザー確認なしには確定できないため、実装は未着手(推測で実装しない)。**
HubSpot への書き込みは一切していない。本物の HubSpot への読み取りは 14 回(すべて Search の POST。読み取り専用。トークンは値を出していない)。

## 1. 「架電の対象」を何で選ぶか — 資料で確定できたこと / できなかったこと

### 確定(資料と実測で裏づけあり)

| 事実 | 根拠 |
|---|---|
| BPO の架電は Deal 中心。パイプライン `753186575` = bpo_リクロジ(実働)、`dealstage=1095387442` = **未済**(架電待ちの在庫) | `~/.claude/skills/hubspot-zoom-slack/references/hubspot-model.md`、Hubspot フォルダ `scripts/bpo_redistribution/generate_candidates.py` の `BPO_PL` / `DEST_UNSHORI` |
| `appointmentscheduled` は default パイプライン(アポ前)の「TEL_未済」。**BPO のキューではない**別パイプライン | 同 hubspot-model.md(英語名から意味を推測しない) |
| 担当者(`hubspot_owner_id`)は日次の再配布で割り当てる。受付ブロック 14 日超・不在 7 日超などは「未済・担当者クリア」で回収 | generate_candidates.py の `STAGES` と docstring、`claudedocs/call_restore_2026-09-02/README.md` |
| 次回架電日 = Deal `bpo_13`(date)、時間 = `bpo_14`(45 択)、最終架電日 = `bpo_20`(date) | 取得済み定義 `docs/research/hubspot-read-foundation/account-property-definitions.json` |
| 停止系 = `bpo_3` 架電禁止理由、`bpo_4` ブロック理由、`bpo_10` 不通時チェック。停止判定は stage も含めて確認が必要 | `account-property-findings.md` |
| ステージ ID の意味(未済 1095387442 / 不通 …443 / 受付ブロック …444 / 不在 …445 / 担当者ブロック …446 / ニーズ4種 / 日程確保 1319310149 / アポ日確定 1095457875 / 案件差戻 …877 / 架電禁止 …878 ほか) | hubspot-model.md |
| 「未済」はリサイクルのリセット先。未済なのに接触履歴・過去アポがあるのは正常 | 同上 |

### 実測(2026-10-04、読み取りのみ。件数は当日の値で恒久ではない)

| 条件(pipeline=753186575 に対して) | 件数 |
|---|---:|
| 全 Deal | 95,886 |
| stage=未済 | 22,976 |
| 未済 かつ 担当者あり | 21,741 |
| 未済 かつ 担当者なし | 1,235 |
| 未済 かつ 担当者あり かつ 次回架電日あり | 179 |
| 未済 かつ 架電禁止理由 `bpo_3` あり | 1 |
| 未済 かつ `bpo_29`(担当者接続電話番号)なし | 22,512(= 電話番号は主に Contact / Company 側を読む必要がある) |
| 次回架電日 `bpo_13` あり(全ステージ) | 9,717 |
| 次回架電日が今日(UTC 0 時)以前 | 1,381 |
| 上のうち未済**以外**のステージ | 1,256 |

- Search の日付フィルタは ISO 文字列を 400 で拒否し、エポック ms(UTC 0 時)が必要だった。日本時間の「今日」との境界(UTC 0 時 = JST 9 時)に注意が要る。
- 示唆: **再架電(次回日が来た)Deal の大半(1,256 / 1,381)は「未済」ではなく不在・ニーズ系などのステージにいる。** 「未済だけ」をキューにすると再架電の約束が出ない。ただしこれが現場のキューの定義かは未確認。
- 未済・担当者ありの先頭 5 件のプロパティ有無を見た限り、`bpo_13` / `bpo_14` / `bpo_29` / `bpo_3` / `bpo_4` / `bpo_10` はすべて空(5 件のみの観察で、一般化しない)。

### 資料からは確定できなかったこと(= ユーザー確認が必要)

BPO 担当者が実際に見ている「架電リスト」は HubSpot の保存ビュー(UI 定義)で、API・手元資料に条件が残っていない。Hubspot フォルダ `docs/hubspot_object_mapping.md` §6 のビュー案(「架電リスト_未済」「架電リスト_再架電: next_call_date ≤ 今日」「架電リスト_BPO: Owner=BPOメンバー」)は設計時点の案で、Contact ベース・旧プロパティ名であり、現行の Deal ベース運用と一致する保証がない。

## 2. ユーザー判断が要る点

**2026-10-05 回答済み。決定は `claudedocs/REACT_HANDOVER_2026-10-05.md` §4.5 を正とする**(Q1: アポ日確定・架電禁止・商談実施処理以外は次回日が来たら出す、Q3: 並び順は画面で切替可能、Q7: 管理者は全員分を見る=`owner=all` を追加し管理者の既定にする、ほかは案どおり)。

| # | 質問 | 候補(推測を含むため採否を確認) |
|---|---|---|
| Q1 | キューに含めるステージは? | 案A: 未済のみ。案B: 未済 + 「次回架電日 ≤ 今日」の Deal(再架電しうるステージの一覧をユーザーが指定。不通・受付ブロック・不在・担当者ブロック・ニーズ4種・日程確保など)。アポ日確定・架電禁止・案件差戻・商談実施処理系・管理パイプラインは除外のはずだが要確認 |
| Q2 | 担当者の絞り込みは? | BPO は「自分(ログイン者のメールに対応する HubSpot owner)が担当の Deal」だけ、でよいか。担当なし(未済 1,235 件)は誰が見るのか(admin のみ?) |
| Q3 | 並び順は? | 案: 次回架電日が今日以前のものを先頭(古い順)、次に未済を最終架電日 `bpo_20` の古い順(空は先)。現場のビューの並びを確認 |
| Q4 | 停止・除外の条件は? | `bpo_3`(禁止理由)あり、`bpo_4`(ブロック理由)あり、`bpo_10`、停止系ステージ(架電禁止 …878 等)を、キューから外す / 灰色表示で残す、のどちらか。電話番号なしは外す / 残す |
| Q5 | 1 Deal に複数 Contact がいるとき、誰に架けるか? | 案: Deal に紐づく Contact のうち関連ラベル「主」があればそれ、なければ最初の 1 件 + 件数表示。電話番号の優先順位は `bpo_29` → Contact `phone` / `mobilephone` → Company `phone` でよいか |
| Q6 | 「今日」の境界は JST 0 時でよいか | Search は UTC 0 時の ms を要求するので換算が必要。案: JST 今日の終わりまでを「来た」とする |
| Q7 | admin(藤巻さん)がキューを見るときの範囲 | owner を指定して見る / 全員分を見る / 見ない |

これらは RBAC 提案書(`C:\dev\hr_crm_oidc\claudedocs\HEADLESS_CRM_ROLES_PROPOSAL_2026-09-30.md`)の「BPO はキューに出たレコードだけ読む」(D-15)と同じ論点。BPO 本人 → HubSpot owner の対応(メール → owner id。Owners API の GET で解決できる見込みだが**未検証**)も要決定。

## 3. 一覧 API の案: `GET /api/crm/call-queue`

認可: 既存 `src/crm/rbac.rs` の `authorize`(Google OIDC 必須 + 許可リスト + 無効アカウント拒否)。レコード単位の絞り込み(自分の owner のみ)は `can_read` の差し込み口に入れる。

### クエリ

| パラメータ | 意味 |
|---|---|
| `limit` | 1〜50。既定 25 |
| `cursor` | 不透明な文字列。応答の `next_cursor` をそのまま返す |
| `q` | 会社名・Deal 名の部分一致(Search の `query`)。最大 100 文字 |
| `stage` | 繰り返し可。Q1 で決まる許可ステージの**部分集合**のみ受理(それ以外は 400) |
| `owner` | admin のみ。`me`(既定)/ owner id / `unassigned`。bpo が指定した場合は無視せず 403 |
| `due` | `today`(次回日が来たもの)/ `all` |
| `sort` | 並び順の切替(2026-10-05 追加)。許可値の一覧だけ受理、既定は §2 Q3 の案 |

### 取得の組み立て(行ごとに API を呼ばない)

1. Deal を Search 1 回(pipeline + 許可ステージ + owner + 次回日、最大 50 件、プロパティは必要最小限)。
2. Deal → Contact、Deal → Company を `batch_associations`(各 1 回、最大 100 件)。
3. 出現した Contact / Company の ID を重複排除して `batch_read` 各 1 回。
4. ステージ表示名は既存の metadata(パイプライン定義、60 秒キャッシュ)から引く。ID を表示名とみなさない。

1 ページ = Search 1 + 関連 2 + 読み取り 2 = 約 5 回の HubSpot 呼び出し(件数に依存しない)。Search は既存クライアントの最小間隔制御(既定 1 req/s)に従う。

### 応答(ts-rs で型生成する形)

```
CallQueueResponse {
  items: CallQueueItem[],
  next_cursor: string | null,
  total: number | null,        // Search の total。参考値
  truncated: boolean,          // Search の 10,000 件上限に達したとき true
  scope: { owner, stages: string[], due },
  partial: { missing_contacts: number, missing_companies: number },
  generated_at: string
}
CallQueueItem {
  deal_id, deal_name, stage_id, stage_label|null, owner_id|null,
  next_call_date|null, next_call_time|null, last_call_date|null,
  stop: { prohibited_reason|null, block_reason|null, unreachable_check|null },
  contact: { id, name|null, phone|null, mobile|null, job_title|null, extra_count } | null,
  company: { id, name|null, phone|null } | null,
  phone_source: "deal"|"contact"|"mobile"|"company"|null,
  deep_links: { deal, contact|null, company|null }
}
```

- 関連が欠けた行は**行を落とさず** `contact: null` 等で返し、`partial` に件数を出す(画面は「担当者情報を取得できませんでした」と表示)。アーカイブ済みの関連先は欠落扱い。
- 失敗時は既存の `error_kind` 体系(`hubspot_rate_limited` 503、`hubspot_timeout` 502 ほか)。**部分的な成功を成功と偽らない**: Deal 取得が失敗したら全体エラー、関連取得だけ失敗したら `partial` を付けて行は返す。

### cursor

HubSpot Search の `after` を、条件(q / stage / owner / due / limit)のハッシュと発行時刻とともに署名した不透明文字列にする。条件が違う cursor は 400 `cursor_mismatch`。Search の `after` は 10,000 件までなので、それ以降は `truncated: true` として止め、絞り込みを促す(owner 単位のキューは数百件の見込みで通常は到達しない。admin の全員分は到達しうる)。

### 画面(次の PR)

- 架空 fixture と実データは**明示切り替え**(モード表示)。実データ取得の失敗時に黙って fixture に戻さない。
- 状態: loading / empty(条件に一致 0 件)/ partial(関連欠落あり)/ error(`error_kind` ごとの文言)/ unauthorized を区別。
- 検索・絞り込みの変更時は `AbortController` で古い要求を中断し、さらに応答の `scope` が現在の条件と一致しなければ破棄する。「さらに読み込む」は同じ条件・同じ cursor にのみ追記する。

## 4. 逆証明(段階2)— 実装時に先に落とすテストの一覧

**今回は設計のみのため未実行。** 偽 HubSpot(既存 `src/hubspot/client_tests.rs` の方式)に対する期待を先に定義した。

| ケース | 期待 |
|---|---|
| 0 件 | `items: []`、`next_cursor: null`、`total: 0`。エラーにしない(画面は empty 表示) |
| 大量件数(total > 10,000) | `truncated: true`。cursor は 10,000 件を超えて進まない。件数を偽らない |
| 関連欠落(Contact なし / Company なし / 両方なし) | 行は残り `contact/company: null`、`partial.missing_*` が実数と一致 |
| batch_read が一部 ID を返さない(207 `errors` のみ含む) | 欠落扱いで継続。全体は成功 |
| 429(Retry-After あり / なし) | 既存クライアントの retry 後も 429 なら 503 `hubspot_rate_limited`。部分結果を成功として返さない |
| タイムアウト | 502 `hubspot_timeout`。トークン・HubSpot 本文が応答・ログに出ない |
| Deal Search 成功 + 関連取得失敗 | 200 + `partial`(行は出る)。画面は partial 表示 |
| アーカイブ済み Deal / 関連先 | Deal は出さない。関連先は欠落扱い |
| 権限なし(未ログイン 401 / パスワードログイン 403 / 許可外 403 / 無効アカウント 403) | 既存 rbac のとおり。**HubSpot を呼ばない**(偽 HubSpot の呼び出し数 0 で検証) |
| bpo が他人の owner を指定 | 403。HubSpot を呼ばない |
| cursor の不整合(条件違い / 改ざん / 期限切れ) | 400 `cursor_mismatch`。HubSpot を呼ばない |
| 検索条件変更中の古い応答(画面) | `scope` 不一致の応答は破棄し、新しい条件の結果だけが表示される(vitest: 遅れて返る旧応答) |
| 日付境界 | JST 23:59 / 00:00 の `bpo_13` が「今日」の内外で期待どおり(UTC ms への換算テスト) |
| 停止系 | Q4 の決定に従う(`bpo_3` あり等) |
| 重複 Deal(ページ跨ぎの更新) | 同一 deal_id が 2 ページに出ても画面側で重複排除 |

## 5. 実装の順序(ユーザー回答後)

1. PR-1: `GET /api/crm/call-queue`(§3)+ 偽 HubSpot テスト(§4 を先に落とす)+ ts-rs 型。
2. PR-2: 画面の実データ化(fixture / 実データの明示切り替え、5 状態、古い応答の破棄)。
3. その後: 本人 → owner の対応(Owners API)と role 本実装(D-15)。

## 6. 今回やっていないこと

- コード変更なし(設計書のみ)。fmt / clippy / cargo test / frontend 検証はコード変更が無いため実行していない。
- 本物の HubSpot の顧客値の取得なし(件数 `total` と、5 件のプロパティ有無の真偽のみ。名前・電話などは取得・記録していない)。
