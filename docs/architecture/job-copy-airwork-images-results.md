# AirWork画像の保存・表示手順（第1段階）

2026-10-12。コードと架空データの確認のみ。Drive保存、HubSpot更新、本番操作は未実施。#84のcollectorとテストを新しいmain上へファイル単位で移植し、実顧客の取得結果資料と古いコミット履歴は移植していない。

## HRハッカーと共通の経路

`job_copy_airwork_images.py` → 非公開の `archive-samples.json` → 既存 `job_copy_archive_batch.py` → 原bytesと求人別manifestをDriveへ不変保存 → Rust `job_copy_airwork_link` → Listing (0-420) の既存 `job_copy_drive_manifest_v1` → `job_copy_live/listings.rs::observed_images` → Rust画像取得 → React。

原本・manifestのhash、Company→Deal→Listing認可、同じ保存receiptの再利用、原本読み戻し、pointer一致/履歴一致時の更新省略は既存HRH実装を使う。Driveの共有権限や保存先設定は変更しない。媒体への都度アクセス・新規DB・LLM呼び出しは追加しない。

差分は公開ページのcollector、59求人単位で進める運用入口、項目を作らない登録CLI、原本確認契約の上限3→100。HubSpotの項目がない/形式不一致なら、Drive保存前に止まる。新規項目の自動作成は行わない。取得元の求人番号・口座を `airworkSource` として不変manifestに保持し、原本読み戻しの前後で、HubSpotの `id_airwork`・`airwork_account_login_id` との完全一致と `kyuujin_status=公開中` を確認する。公開ページ404だけを根拠に掲載終了とは断定しない。

## 非公開入力

既存の取得済み求人一覧から、口座・採用サイト・媒体求人・Listing・Companyの対応を確認済みのファイルを作る。`kyuujin_status` は正規化済み値をそのまま渡し、`公開中` だけを対象にする。公開終了/掲載終了/公開前/不明は除外し件数を別に出す。生状態01/02や欠損を公開中へ推測変換しない。

```json
{"schemaVersion":1,"jobs":[{"accountKey":"synthetic-account","recruitSiteUrl":"https://arwrk.net/recruit/synthetic-account","mediaJobId":"123","hubspotListingId":"30","companyIds":["10"],"kyuujin_status":"公開中"}]}
```

`accountKey` は取得元口座を識別する値として、HubSpotの `airwork_account_login_id` と厳密一致するものを渡す。キャッシュが表示名などをキーにする場合は、取得済みの口座対応からログイン識別情報の一致を確認した対応表を用意し、別口座から推測しない。採用サイトURLを省略し `--recruit-cache <既存非公開aw_recruit_urls.json>` で照合してもよい。`expectedImages` は確認済みの枚数のみ。未取得は省略/null。既存画像参照を更新する場合は、その現在参照から `previousManifestFileId` / `previousManifestSha256` を入力へ足す。既存参照があるのに前参照がない場合は、既存の移行検証が登録を拒否する。

**実測対象件数は未取得**。この作業場所では既存非公開一覧とURL対応表を発見できず、提供先を確認中。約1,000件は依頼時の想定であり、実測ではない。実在の掲載中求人3件の公開GET確認も未実施。この制約をPRの未完了項目として明記する。

## 第2段階のコマンド（今回実行しない）

準備: 本PRのコードを使い、`CARGO_TARGET_DIR=<専用target> cargo build --example job_copy_airwork_link` で登録バイナリを作る。Python環境にrequests、Pillow、google-authと既存gwsを用意する。既存HRH保存設定・同じreceiptディレクトリ・サービスアカウント鍵・HubSpot環境ファイルを非公開で指定する。

取得から登録までの実行は次の2本。まず `--apply` を外して検査し、件数・対象対応・既存項目の確認後に実行する。取得の検査は完全オフラインで、保存の検査もローカル原本のhashだけを検証する。保存の実行開始時には登録CLIの読み取り専用 `--check` が既存項目を確認する。

```sh
python scripts/job_copy_airwork_pipeline.py --input <非公開求人一覧.json> --recruit-cache <非公開採用サイト対応.json> --output <非公開実行ディレクトリ> --phase capture --apply
python scripts/job_copy_airwork_pipeline.py --input <同じ求人一覧.json> --recruit-cache <同じ採用サイト対応.json> --output <同じ実行ディレクトリ> --phase store --storage-config <既存保存設定.json> --service-key <既存SA鍵.json> --env-file <既存HubSpot.env> --link-bin <専用target/debug/examples/job_copy_airwork_link> --apply
```

1〜2本目を同じ入力・出力・receiptで再実行する。完全取得済みの求人は原本hashと入力対応を検証して再利用し、失敗/途中停止の求人だけ再取得する。取得後は媒体を読みに行かない。同bytesの画像は既存バッチが内容hashで排除し、予約済みDrive IDを再利用する。登録は同pointer/前参照の履歴一致ならPATCH0。応答喪失時も現在参照を読み直して一致で完了判定する。二重登録を防ぐ前提として、receiptを消さず、同じ保存設定で単一実行者が動かす。

入力が変わった場合は既存実行ディレクトリを再利用できない。新規実行先を用意し、保存設定のreceipt場所は引き継ぐ。単なる再取得で新しいobservedAtのmanifestを作り直さない。未知の画像数・失敗を0枚の成功へ変換しない。公開ギャラリーを確認して画像0枚だった求人はローカル証跡のみで、空samplesをDrive保存へ送らず、画面上の登録なしとは断定しない。

取得不可は1行で表示し、終了コード2で部分失敗を示す。保存済みの完全取得分のみ処理できるが、元入力全件の完了とは扱わない。登録エラーはそのバッチで停止する。ログは連番・集計・理由のみで、口座・顧客名・媒体URL・画像・生IDを出さない。非公開結果には対応情報が必要なので、Gitへ追加しない。

## 想定件数・時間・書き込み・停止

- 想定1,000求人なら17バッチ（59求人以内）。実測件数は未取得。
- 求人当たり画像を仮に4枚とすると、公開GETは約5,000回、原本4,000点以内＋manifest1,000点以内。重複bytesは共有する。実画像数と容量は未取得。
- 時間は実測未取得。GETを仮に0.5〜2秒/回、Drive保存・原本検証を仮に1〜5秒/画像、manifest/登録/関連確認を仮に2〜10秒/求人と置くと、約2.4〜11.1時間。429・遅延・失敗再開を含まない概算であり、3求人の実測後に更新する。並列数は増やさない。
- 初回のHubSpot PATCHは画像があり完全取得できた求人数まで（想定上限1,000回）。既存一致時は0回。項目CREATEは0回。GET/関連確認やDrive読戻しは別に発生する。
- 止め方はCtrl+C。原本、求人checkpoint、Drive予約receipt、登録結果を削除しない。子の登録処理が停止したことを確認してから同じコマンドで再開する。書き込み応答が不明でも新IDで作り直さず、既存参照と原本を再検証する。
- 新しいHubSpot項目は不要という設計。既存項目が未作成なら作らず停止し、既存の `job_copy_drive_manifest_v1` と同契約の項目を使う案を別途レビューする。

## 検証

Pythonは模擬HTTP/保存/登録で検証。Geminiや実APIキーは使用しない。Rustは不変参照のhash・顧客関連・5枚の順序・5枚目の取得契約・項目欠損の書き込み0を検証。Reactは5枚/終了/未取得とHRH既存テスト、E2Eは1440×742・1920×742で本文/ポップアップの5枚全画像を確認し、媒体GET0を検証する。

スクリーンショット: [求人内容と5枚](../screenshots/job-copy-airwork-images/01-body-five-images.png)、[ポップアップ](../screenshots/job-copy-airwork-images/02-popup-five-images.png)、[掲載終了](../screenshots/job-copy-airwork-images/03-ended-unavailable.png)。いずれも架空の求人と作図素材のみ。

ローカル検証結果: PythonのAirWork取得・再開29件、既存Drive54件、既存保存5件（非公開証跡依存1件は対象外）、React全2064件、型検査・lint・build、AirWork E2E 2件と既存ポップアップE2E 4件を確認。Rust対象テストとcargo fmt --check、登録exampleのコンパイルも確認。CIの確定結果はPR参照。


## レビュー対応（取得元照合）

取得元の求人番号・口座が保存されず、同じ顧客の別求人を登録できる指摘を修正した。collectorの各sampleに `airwork_source` を付け、既存保存バッチが検証してmanifestの `airworkSource` へ引き継ぐ。求人内のslotで取得元が異なる場合は保存前に停止する。操作識別値にも取得元が含まれるため、異なる取得元の保存記録を再利用しない。

Rustの専用登録はhash検証済みmanifestの取得元を必須にする。共通登録でも取得元があれば同じ照合を行い、取得元を欠いた証跡はAirWork求人へ登録できない。旧HRHの読み取りは変更せず、旧形式の登録はHRハッカーと確認できる求人だけに限定する。求人番号不一致・口座不一致・欠損・公開終了はPATCH0。原本読み戻し中に取得元が変わった場合も、登録直前の再確認で停止する。HubSpot側には条件付き更新がないため、既存どおり単一実行者で運用する。

古いAirWorkの取得済みcheckpointへ取得元を推測補完しない。sourceを欠いた古いsamplesは再開時に拒否するため、再取得には新しい実行ディレクトリを使う。既存保存設定の画像receiptは保持する。第2段階はこの新しい読取契約を含むアプリコードの反映後にのみ実施する（今回の反映・外部書き込みは行わない）。

実測用の非公開一覧と採用サイト対応表は再探索でも確認できず、提供先を再確認中。掲載中の実測件数と実在求人3件の取得確認は、引き続き**未完了**であり、この指摘を解消済みとは扱わない。
