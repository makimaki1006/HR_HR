# HRハッカー・AirWork：求人データ取得ロジックの引継ぎ

更新日：2026-10-08。媒体取得担当とHR_HRアプリ担当が同じ原本・観測結果を使うための契約。GitHubへの連携窓口は [HR_HR PR84](https://github.com/makimaki1006/HR_HR/pull/84) に集約する。

## 1. 所有境界と参照する実装

求人・応募の同期正本は [Hubspot_Job-Posting](https://github.com/sfujimaki-art/Hubspot_Job-Posting)。2026-10-08にmain `c58aed0c864abca5b2fabb5bc8b1541ca909a832` とfetcherファイルの存在を確認した。手元の別originのHubSpot checkoutを本番正本として優先しない。次担当は実装前に現在main・配備・ジョブを照合する。

|工程|HRハッカー|AirWork|
|---|---|---|
|本文原本の取得|`scripts/job_application_sync/fetchers/hr_csv_fetcher.py::fetch_hr_csv`|`scripts/job_application_sync/fetchers/aw_csv_fetcher.py::fetch_aw_xlsx`|
|対象の決め方|管理認証で閲覧できる店舗群。顧客・店舗台帳と件数を照合する|`fetchers/account_loader.py::iter_aw_accounts`で顧客台帳を読取、企業B系IDで重複排除|
|原本形式|媒体の求人CSV|ZIP内の求人XLSX|
|識別キー|媒体求人ID＋店舗ID|媒体求人ID＋企業アカウントキー|
|画像の参照取得|CSVの画像1〜3のURL列|口座別採用サイトURL＋XLSX求人ID→公開求人ギャラリー|
|画像bytesの取得|HR_HR `scripts/job_copy_images.py::capture`|HR_HR `scripts/job_copy_airwork_images.py::capture_jobs`|
|保存・画面への接続|両媒体共通：原本→既存Drive不変manifest→HubSpot pointer→Rust認可読取→React|

HubSpotをCRMのSystem of Recordとし、Rust/Axumを認可境界、Reactを既存アプリ内の表示とする。新しいCRMマスターDBは追加しない。媒体取得にHubSpot更新・応募取込・通知を混ぜない。

## 2. HRハッカーの取得

1. 既存管理認証をサーバー側の環境設定から読む。認証情報をブラウザーやGitへ渡さない。
2. `fetch_hr_csv(output_dir, is_valid='', headless=True)`を使う。空の`is_valid`は全状態を要求するが、管理認証の閲覧範囲を超えて取得できるという意味ではない。
3. 管理画面の一覧フォームからCSV生成を要求し、完了したexportの原本を保存する。
4. CSVは媒体の列をそのまま保持し、本文は仕事内容だけでなく関連する文面列も原文から構成する。生成用84列テンプレートの検証を、実際にダウンロードしたCSVの取得成功と混同しない。
5. 求人ID＋店舗IDでHubSpot Listingを対応させ、関連Companyを確認する。求人ID単独で店舗をまたいで結合しない。
6. 画像1〜3の参照URLを順序付きで抽出し、本文の差分有無にかかわらず今回のbytesを取得する。

現行HR_HRの`job_copy_images.py`は選定済み`selected.json`が対象で、全CSVの画像を自動的に収集する処理ではない。許可AWS host、redirect禁止、8MiB・3,000万画素・JPEG/PNG/WEBPの検証を経て原bytesをSHA別に保存し、表示用900px JPEGを別に生成する。履歴の正本は再圧縮した表示版ではなく原bytes。

### 鮮度を判定する際の注意

取得済みファイル名の日付やmtimeを、媒体の生成日・求人変更日として使わない。完了exportを選ぶ処理が今回の生成要求を識別できるかは、現在のfetcherで確認する。過去の完了行を今回の生成結果として取得する反例をテストする。

正本の[現行HR fetcher](https://github.com/sfujimaki-art/Hubspot_Job-Posting/blob/c58aed0c864abca5b2fabb5bc8b1541ca909a832/scripts/job_application_sync/fetchers/hr_csv_fetcher.py#L404)でも、完了行の選択時に今回のkick時刻を比較しないことを静的確認した。このため「新しい生成要求をしたので取得CSVも新しい」という保証はまだできない。

最低限、生成要求の識別子・開始日時、選んだexportの識別子・媒体生成日、取得日時、原本SHA、行数を分けて記録する。生成日の根拠が取れない場合はunknownを維持する。日付単位の観測でよく、正確な求人変更時刻を推定しない。

## 3. AirWorkの全アカウント取得

### 台帳と対象

企業B系ID/PWだけを使用する。RPO側A系をログインのfallbackにしない。シートは1回読み、ログインIDの完全一致で重複排除する。同一IDに異なるPWがある場合は勝手に選ばず保留する。

2026-10-07の実対象は資格情報あり561行→519アカウント、重複42行、PW競合0。既存loaderの`active_only=True`はクローズTRUEだけを除外するため、対象には状態未指定1行も含まれる。全台帳の企業ログインID900種類や、HubSpot検索に現れた404口座と同一視しない。

資格情報がない実値あり182行、クローズ済み資格情報あり402行は別管理。チェックボックスだけの空相当行を顧客数に加算しない。資格情報未登録は媒体未利用・整備不足などの確認が必要で、取得成功や削除と扱わない。

### 取得と並列化

1. 応募発生を条件にせず、全対象にpendingを作る。
2. CSV取得4 workers、画像取得2 workersで同時実行する。各アカウントのsession/cache/outputを分離し、同じ口座の重複ログインを防ぐ。
3. `fetch_aw_xlsx(..., mode='full', storage_state_path=口座専用path)`はログイン→必要ならexport生成→ダウンロード。`mode='collect'`は生成済みexportの回収だけで、生成未完了は再待機対象になる。
4. ZIP内XLSXの数・拡張子・展開サイズを検証して全列・本文を保持する。CSVだけ成功した口座から画像キューに投入し、全CSVの完了を待たない。
5. 原本SHA、本文全行/全列、件数を照合した成功分だけ再利用する。再開時に成功分を再ダウンロードしない。画像収集中の停止はinterruptedとして残す。
6. 台帳更新はイベントループ内の単一writer。画像threadsは口座/実行/batch専用ディレクトリだけに書く。全519件を多数processで同じJSONへ書かせない。

正本の[現行AW fetcher](https://github.com/sfujimaki-art/Hubspot_Job-Posting/blob/c58aed0c864abca5b2fabb5bc8b1541ca909a832/scripts/job_application_sync/fetchers/aw_csv_fetcher.py#L347)でも生成済みexportの再利用、[口座loader](https://github.com/sfujimaki-art/Hubspot_Job-Posting/blob/c58aed0c864abca5b2fabb5bc8b1541ca909a832/scripts/job_application_sync/fetchers/account_loader.py#L185)でもB系のみ使用・クローズTRUEだけ除外することを確認した。

本文の対応は`求人番号(job_offer_id)`、画像申告数は`募集イメージ数(photo)`。画像枚数の空欄はunknown、0を補完しない。XLSXの生成日は原本`docProps/core.xml`のcreatedで確認できる場合がある。ライブラリが補ったmodifiedやダウンロード時刻だけで生成日を証明しない。

### 公開画像の取得

口座別`aw_recruit_urls.json`のキーを厳密照合して`https://arwrk.net/recruit/<site>/<job>/`を作る。採用サイトとアカウントの対応が曖昧なら保留する。公開ページのcanonicalがその求人と一致することを確認し、`.job-img .slider`の画像をslot順に取得する。thumbnail・ロゴ・slide cloneは除く。

`scripts/job_copy_airwork_images.py`はHTTPS/固定host/path、redirect禁止、page2MiB・画像5MiB・3,000万画素・100画像を検証する。1バッチ59求人に分割するが、媒体の全件母集団を59件に切り詰めない。実CDNのoctet-streamはPillowによる実形式確認後に受け入れる。

404/410、HTML構造未対応、画像HTTP失敗、申告枚数不一致は別の欠測理由。公開画像を取得できない求人の本文を一覧から消さない。非公開・終了求人の画像を認証済み管理画面から読むadapterは未実証。公開404だけで求人削除と決めない。

## 4. 両媒体共通の受渡し契約

取得担当は以下を非公開artifactとしてアプリ担当へ渡す。原本や顧客一覧を公開GitHubに添付しない。

```json
{
  "schemaVersion": 1,
  "media": "airwork",
  "scope": "managed_accounts",
  "sourceGeneratedDate": null,
  "sourceGeneratedDateEvidence": null,
  "acquiredAt": "<RFC3339>",
  "observedDateJst": "<YYYY-MM-DD>",
  "sourceRecordChangedAt": null,
  "raw": {"privateLocation": "<private artifact>", "sha256": "<raw SHA256>", "bytes": 0},
  "jobs": [],
  "coverage": {"expectedAccounts": 0, "successfulAccounts": 0, "emptyAccounts": 0, "failedAccounts": 0, "pendingAccounts": 0},
  "complete": false
}
```

これは次担当へ渡す契約例で、既存fetcherがこのJSONを直接返す実装は未完了。数値0・空jobsは合成例で、実データの欠損を0に変換する規則ではない。

求人ごとに媒体複合キー、元列、本文、掲載状態、Listing/Company対応または未対応理由、画像申告数/観測数/成功数/欠測理由を保持する。画像はslot、URL参照SHA、原bytes SHA、形式、サイズ、取得日、非公開配置を保持する。本文と画像の観測時点は独立して記録する。

対応が確認できた原本だけ既存`job_copy_archive_batch.py`→Drive不変manifestへ渡し、HubSpot専用pointerを更新・読み戻しする。Driveへ原本、HubSpotへfile ID/SHA/観測日だけを置く。Rustの認可を通してReactが画像を読み、一般公開Drive URLや失効する媒体URLを履歴の正本にしない。

毎回の観測で先に現在画像を保存し、前回成功原本と比較する。画像変更の検知後に過去画像を取りに行く構造にしない。同URLのbytes変更、URLだけの変更、順序変更、A→B→Aを区別する。観測間に消えた画像を後から復元できると保証しない。

## 5. 失敗・再開・計測

- account outcomes：pending / downloading / downloaded / empty / failed。画像はcomplete / partial / unavailableを別記録する。
- 全対象数＝取得成功＋求人なし＋取得失敗＋未着手/実行中をキーで照合する。CSV成功と画像全成功を混同しない。
- 失敗理由は発生箇所で静的コードにする。auth form/submit/redirect、export生成/完了/クリック、HTTP429/5xx、timeout、schema、保存容量、関連不明を分ける。PWやCookie、raw例外文をログへ出さない。
- 今回の209口座はRuntimeErrorまでしか保存しておらず、個別原因は未特定。過去結果へauth_failedを推測補完しない。認証失敗を無制限に再ログインせず、再試行可否を判断する。
- GET一時障害は有限回数・backoffで再試行する。公開ページを読めない場合は最後の成功画像を日付付きで保持し、当日成功と表示しない。
- 全体開始、CSV巡回終了、画像巡回終了、全体所要秒、口座/試行別所要秒を記録する。全体wall時間は再起動中の停止時間を含める。
- ローカルoperatorのJSON台帳/OS lockは取得検証用。本番の耐久Pending/Retry、単一writer、日次起動、電源断耐久の完成証拠ではない。HubSpot書込み失敗の本番保存方式は既存ADRの未決事項を維持する。

## 6. 今回の実測と未完了事項

2026-10-07 19:31:40〜20:56:31（日本時間）、実測84分51.255秒。全519アカウントの一巡は終了した。

|結果|実測|
|---|---:|
|CSV取得成功|302アカウント、5,795求人|
|求人なし|8アカウント|
|CSV取得失敗|209アカウント、個別原因未特定|
|画像観測|5,795求人|
|公開ギャラリー取得成功|805求人|
|公開ページ取得不可|4,700求人|
|画像/ページHTTP失敗|208求人|
|HTML構造未対応|80求人|
|申告画像数との不一致|2求人|
|保存画像参照|1,760件|
|重複排除後の画像原本SHA|1,196種類|
|口座内の画像全求人成功 / 部分取得|34 / 268アカウント|

519＝302＋8＋209、5,795＝805＋4,700＋208＋80＋2。画像参照1,760件は原本の重複排除後の種類数ではない。2026-10-08に302原本のSHA/本文行数、画像1,760参照のSHA・サイズ・形式とHTML証跡SHAを再照合した。失敗209アカウントの顧客名付き一覧は非公開のまま保持する。

この巡回ではHubSpot/Driveへの書込み、アプリのオンライン表示更新は0。HRハッカーの全管理店舗を今回再取得した実測ではない。日次の全口座観測、未取得口座の解決、全媒体の自動反映は未完了。

次担当は取得CSVの完了フックをこの契約へ接続し、管理対象・未対応キー・鮮度を揃え、原本保存→pointer登録→認可読取→本文先行/画像遅延表示まで実証する。媒体ダウンロードをアプリ側で重複起動しない。

## 7. 検証と参考資料

必須反例：同ID別店舗/口座、対応曖昧、古い完了CSV、mtimeだけ変更、画像枚数空欄、同URL別bytes、失効前原本保存、取得不能を削除扱いしない、中断再開、成功原本の改変、失敗を除外した偽の全件成功。

今回のAirWork collectorの単体・Drive接続契約テストと、4＋2の合成並列検証は、全対象の媒体ログイン成功・Drive登録・本番画面成功を証明しない。Rust/Reactはこの引継ぎ変更で改変しない。

- [全求人連携の引継ぎ](job-copy-all-listings-handoff.md)
- [AirWork collectorの実装と呼出し](job-copy-airwork-images-results.md)
- [全口座収集と計測の記録](job-copy-all-account-acquisition.md)
- [Drive保存契約](job-copy-drive-storage-contract.md)
- [本文先行・画像遅延](job-copy-loading-performance-plan.md)
