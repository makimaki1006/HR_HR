# HubSpot 接続と実プロパティの確認結果

確認日: 2026-10-01。HubSpotフォルダの設定資料と、既存資格情報による公開Properties APIのGETを確認した。今回の取得はプロパティ定義のみで、顧客レコード値の取得・更新・発信は行っていない。

## 接続設定と権限

- 参照フォルダ: `C:/Users/fuji1/orca/workspaces/Hubspot/silverside`。`.git` の参照先から元リポジトリを確認した。
- `CLAUDE.md` と `docs/hubspot_api_access.md` は、RESTアクセスにService Keyを使う運用を記録している。Developer Projects / Static Authという古い記述も残るため、今回成功したREST接続を基準にする。
- `src/config.py` の環境変数は `HUBSPOT_ACCESS_TOKEN` と `HUBSPOT_PORTAL_ID`。作業用フォルダに実 `.env` はなく、元リポジトリにある。トークンはメモリ内のAuthorizationヘッダーだけに用い、値を出力・転記していない。
- 同フォルダの記録には、2026-04-09/16時点でContact / Company / Dealのread/write、schemas、owners、users、lists、import/export、sales-email-readなど19scopeがある。これは資料上の記録であり、現在の全scopeをAPIで再検証した結果ではない。
- 今回確認したのは4objectの定義GET成功。書き込み権限、レコード範囲、Workflow、BPO利用者の権限は未検証。Service Keyの権限をBPO本人の編集権限として扱わない。
- HR_HR側には実トークンをコピーしていない。実接続時はRustのサーバ環境へ設定し、必要な権限と許可項目を限定する。トークンのローテーション・失効を扱い、有効期限がないという旧資料の記述を恒久保証とはしない。

## 定義APIで確認した範囲

`GET /crm/v3/properties/{objectType}` を各1回実行し、いずれもHTTP 200。

| Object | 取得した有効プロパティ定義数 | 記録に残した関連定義数 |
|---|---:|---:|
| Contact | 419 | 8 |
| Company | 386 | 10 |
| Deal | 1,339 | 74 |
| Call | 124 | 6 |

[定義の抜粋](account-property-definitions.json)に、取得日時・総数・関連するinternal name・label・型・選択肢・readOnlyValueを保存した。全定義や顧客の実値をこのリポジトリへ複製しない。API仕様の参照は [HubSpot公式Properties API](https://developers.hubspot.com/docs/api-reference/legacy/crm/properties/guide)。

## 画面との対応候補

以下の内部名・型は実定義で確認した。一方、どのタイミングで必須・更新するか、Workflowの発火条件、類似項目の使い分けは未確認。存在することと採用すべきことを区別する。

| UIで扱う情報 | Objectと実internal name | 実定義・変更点 |
|---|---|---|
| 次回架電 | Deal `bpo_13`、`bpo_14` | 日付と時間を分離。時間は45選択肢。単一datetime入力から保存先への変換が必要 |
| 会話した相手 | Deal `bpo_40`、`bpo_21`、`bpo_22` | 接触結果は受付/担当者。氏名の漢字・よみは別項目。Contactの氏名変更とは区別 |
| 温度感 | Deal `bpo_42` | 高（前向き）/中（検討余地あり）/低（否定的）/聞く耳なし。デモの3択とは異なる |
| 次アクション | Deal `bpo_45` | 再架電/メール送付/資料送付/日程調整打診。デモの引き継ぎ/対応終了と1対1ではない |
| 募集職種 | Deal `bpo_24` | string/text。会社単位ヒアリングの仮定を見直す |
| 募集人数 | Deal `bpo_49` / Company `planned_number_of_hires` | いずれもstring/text。アポ用と会社の採用予定人数の使い分けを確認。両方へ自動反映しない |
| 採用課題 | Deal `bpo_25` | 8択の複数選択。自由記述からチェック式へ。その他詳細の保存先も確認 |
| 補足・メモ | Deal `bpo_8`、`bpo_16` / Call `hs_call_body` | string/textareaとCallのhtml本文。一般メモ/タスクメモ/通話履歴を区別し、同じ内容を無条件に全箇所へ送らない |
| 架電停止・ブロック | Deal `bpo_3`、`bpo_4`、`bpo_10` | 禁止理由、ブロック理由、不通時チェックは別項目。停止判定はpipeline/stage等も含めて確認 |
| アポイント | Deal `bpo_23`、`bpo__`、`bpo_33` | 商談予定日、商談予定時間、商談方法。現デモの「アポイント獲得」だけでは情報が不足 |
| 募集種別 | Deal `bpo_34` | 正社員/パート/契約社員/未聴取の複数選択 |
| 接続先電話番号 | Deal `bpo_29` / Contact `phone`・`mobilephone` / Company `phone` | 発信先の優先順位・取り次ぎと直通の使い分けを確認 |
| 担当者の役職 | Deal `bpo_50`、`bpo_9` / Contact `jobtitle` | アポ用自由記述、役職選択、Contactの役職が併存。更新先を業務で決める |
| 採用開始時期 | 未確定 | 現デモの自由入力を保存する先は、今回の抜粋だけでは特定できない。別項目を推測で割り当てない |
| 通話結果 | Call `hs_call_disposition` | enumeration/selectだが今回の定義応答のoptionsは0件。選択肢なしとは扱わず、別の結果定義取得を接続設計で確認 |

複数選択の内部値やdateのAPI表現は、[公式仕様](https://developers.hubspot.com/docs/api-reference/legacy/crm/properties/guide)に従ってサーバで変換する。表示ラベルと内部値が一致しない項目がある。例: `bpo_10` の「現在使われておりません」は内部値「使われておりません」、`bpo_4` の「リスト被り（架電被り）」は内部値「リスト被り」。

## 権限設計で注意する点

`bpo_18`（掲載元 ※編集不可）、`bpo_19`（ハロワ更新データ ※編集不可）、`bpo_32`（URL_求人検索 ※編集不可）は、APIのreadOnlyValueがfalse。業務上の編集禁止は、このAPIフラグから自動判定できない。UIとRustの許可リストで参照のみにする。

同様に、入力者・累積接触数・自動計算/日付などをAPI上書き可能だからといって編集欄にしない。BPOの役割と、元の同期・Workflow・シートの主たる書き手を照合する。

## 実装計画への反映

一覧の単位をContactだけで固定せず、Dealを中心にした架電対象が適切かを既存リスト/ビューで確認する。同じ会社に複数のDealがある場合、ヒアリング下書きをCompany IDだけで共有すると別案件へ混ざるため、Deal項目はDeal IDで分離する。会社基本情報のみCompany IDで共有する。

初期の詳細入力は、接触結果・温度感・次アクション・次回架電・職種・人数・課題・アポ情報を中心に候補を絞る。全1,339項目を画面に並べない。現行画面はまだ仮のデモ項目であり、今回の定義を使う実入力・実保存は未実装。

次はHubSpotフォルダのリスト・pipeline・Workflow資料と同期マッピングを確認し、BPOの必須条件と保存先を確定する。顧客の実値を照合する段階では、対象ビューと最小限のレコードを選ぶ。
