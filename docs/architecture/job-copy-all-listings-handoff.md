# 全求人票連携：別セッションへの引継ぎ

作成日：2026-10-06、取得ロジック更新日：2026-10-08。目的は限定MOCを、管理対象の全求人票の本文・画像・観測履歴・応募集計が見られる既存アプリへ拡張すること。作成当初は文書整理だけだった。後続のAirWork取得結果は末尾の更新記録と [両媒体の取得契約](job-copy-media-acquisition-handoff.md) を参照し、原本取得と本番反映を区別する。

## 別セッションに渡す依頼文

> HR_HRの求人文面管理を、現状の36求人の固定スナップショットから全管理対象求人の連携へ拡張してください。この文書を最初に読み、HubSpot側の求人・応募同期担当と同じ取得結果を再利用してください。既存Rust/Axum・React画面・Google Workspace認証・HubSpot正本・Drive画像保存を維持します。収集、画像保存、HubSpot参照登録、アプリ読取、件数照合までが担当範囲です。求人本文の編集や新しいCRMマスターDBは作りません。全件の定義と実件数、最新の同期契約を確認し、実装計画を作ってから段階的に進めてください。上限59の解除だけで全件対応済みとしないでください。統合・配備は既存のリポジトリ連携窓口に一本化してください。

## 1. 引継ぎ時点の正本と作業場所

| 対象 | 状態・参照先 |
| --- | --- |
| アプリのリポジトリ | `makimaki1006/HR_HR`。最新mainを取得して開始する |
| 今回までの統合 | PR58系の求人管理に加え、PR73で本文先行・画像遅延読取、PR75で待機・再取得を追加。確認したmainは `f58d6bedc45161a3d700a55e01fbf6149241671e` |
| オンライン画面 | `https://hr-hw.onrender.com/app/job-copy`。既存アプリ内のReact画面 |
| 本書作成用checkout | `<HR_HR_WORKTREE>`。本書以外のコードは上記mainに統合済み |
| 元の調査・収集・非公開証拠 | `<ORIGINAL_HR_HR_WORKTREE>`。多数の別作業差分がある。丸ごとstage・コピー・checkoutしない |
| HubSpot側の調査資料 | `<HUBSPOT_CHECKOUT>/claudedocs/job_copy_return/` の01・02・03文書 |
| HubSpot同期の正本 | 返却資料では `sfujimaki-art/Hubspot_Job-Posting` main `0b38296`（2026-10-04時点）。現在のmain・配備・workflowは引継ぎ先が再確認する |

HubSpot返却資料にある `C:/dev/jas_link` は今回の端末では確認できなかった。DesktopのHubspot checkoutはHEAD `e5ec583`で未コミット変更もあるため、本番の正本として優先しない。以前の `job-copy-react-join` 作業フォルダは削除されており、参照先にしない。

最初に `AGENTS.md`、`CLAUDE.md`、`frontend-react-migration.md`、`headless-crm-design.md`、`architecture-decisions.md`、`react-full-migration-plan.md` を読む。本文・画像・応募の保存方法を変える前に現行ADRと比較する。

## 2. 現状でできていること／全件化が未完了なこと

画面の実データは取得時点の **36求人・320応募レコード**。日次最新値ではない。画像は **28求人・45参照・34種類の原本**をDriveへ保存し、HubSpot pointer登録とRust経由読取を確認した。過去画像の保存成功をこの件数から推定しない。応募動機12記述、他の理由2出典は未記録。応募の日付仮対応3件・版対応不明317件で、理由の版対応は全件不明。

一部古い文書の317応募・2求人/3画像・未配備という記述は、その時点の検証結果。最新36/320版、PR73・75の実装を優先する。ただし本番Googleログイン後の全文・画像表示と速度の測定はまだ実証していない。

未完了：全対象の棚卸し、全件本文・画像収集、日次保存、収集→アプリの自動更新、運用用の永続Pending/Retry、複数writer制御、AirWork画像取得、全コンサルタントの閲覧権限設定。HR課金実績は取込契約とUIがあるが実データ未接続。LLM検索・文面案生成は将来機能。

### AirWorkが一覧から見えなくなった理由（2026-10-07確認）

初期の操作デモにはAirワークの架空求人がある。一方、実データへの切替後に使用した20求人版・36求人版・Drive表示用版はすべてHRハッカーのみ。元gurnardの `job_copy_select_applicant_review.py` はListingの `id_hrhakkaa` と `id_shop_hrhakkaa` だけを読み、HR CSVに一致する候補を採用する。`job_copy_build_live_capture.py` も媒体をHRハッカーとして生成するため、AirWorkが実データ一覧に入る経路がない。これは今回の表示データ選定の不足であり、HubSpot上のAirWork求人の現存・削除状態を確認した結果ではない。

AirWorkは1アカウント52求人の実XLSX取得と本文列を以前に確認済み。ただし全口座の取得やHubSpotとの全件対応は未確認。**優先してAirWorkの本文・条件・求人関連を表示用データへ接続し、画像は「未取得」として表示する。画像取得完了を求人本文の掲載条件にしない。** 初期の架空AirWork求人を実データとして戻すことはしない。

2026-10-07追加：`scripts/job_copy_airwork_images.py` に既存口座別採用サイトURLキャッシュを使う公開求人ギャラリー取得を実装。1口座の3公開URLを確認し、1求人のJPEG原本1件（31,278bytes）を保存・hash照合、他2求人は404として保持した。反例21テスト、既存Drive54テストとarchive6テスト（1skip）を確認。画像原本/slot/hash/取得日と既存Driveバッチ用samplesを出す。Driveアップロード・HubSpot登録・定期起動・非公開求人の管理画面取得・現行UIの4画像以上表示は未接続。次担当は [実装と呼出し手順](job-copy-airwork-images-results.md) を読み、取得完了フックへ接続する。

取得ロジックの統合用draftは [PR84](https://github.com/makimaki1006/HR_HR/pull/84)、branch `feat/job-copy-airwork-images`、commit `aba1083`。本番へ反映済みではない。元gurnardにも実装・21テスト・計画・利用手順を配置し、同じテストを成功確認した。

## 3. 現在のデータの流れ

1. HubSpot側の媒体取得がHRハッカーCSV／AirWork ZIP内XLSXを取得する。既存求人・応募同期は別プロジェクトの所有。
2. HR_HR側の限定収集ツールが媒体の本文・条件・画像参照を求人キーでHubSpot Listingへ対応させる。本文は媒体の複数列から全文を組み立てる。HubSpot `shigotonaiyou` 単独を媒体全文と扱わない。
3. 観測時に画像bytesを取得し、原本hashで重複を排除して非公開Driveへ保存する。求人別不変manifestを作成する。
4. HubSpot Listing専用プロパティ `job_copy_drive_manifest_v1` にmanifestのDrive file ID・原bytes SHA-256・観測日時だけを登録する。
5. 本文・履歴・応募集計を組み立てた不変レビューJSONもDriveへ保存し、サーバーのfile ID/SHA設定を対で切り替える。現在はoperator操作で、日次同期には未接続。
6. ReactはRust `/api/job-copy/moc` から本文・応募集計を先に取得する。画像は表示時に `/api/job-copy/snapshot-image` → 既存 `/api/job-copy/image` へ遅延解決する。

ブラウザーからDriveやHubSpotの資格情報を使わない。画像の取得主体はRust。Google Driveの一般公開リンクや媒体URLをそのまま履歴表示の正本にしない。

## 4. キーと関連の契約

| 対象 | 使用するキー・項目 |
| --- | --- |
| 顧客／契約 | Company `0-2` → Deal `0-3` → Listing `0-420` の全関連。会社名一致や先頭関連だけで決めない |
| HRハッカー求人 | `id_hrhakkaa` と `id_shop_hrhakkaa`。8桁求人IDは先頭ゼロを保持した文字列。求人ID・店舗ID・HubSpot IDは別物 |
| AirWork求人 | `id_airwork` と `airwork_account_login_id`。口座を落としたID単独結合をしない |
| 応募 | Appointment `0-421` → Listing関連、`yingmuri`、`oubobaitaimei`、`oubokyuujinmemo` |
| 応募属性 | `seibetsu`、`nenrei`、`todoufuken`、`shikuchouson`。欠損は不明群で保持 |
| 応募理由 | `oubodouki`、`ouboriyuu_baitaikisai`、`ouboriyuu_hiaringu` を別出典として扱う |
| パイプライン／ステージ | HubSpot実schema・pipelinesから名称を確認。表示ラベルを推測して固定しない |

返却された01文書では、同じ取引先コードの生きている主契約すべてへListingを関連付け、旧関連は削除しない方式へ変更済み。`deal_master.link_targets` と `latest_live` の用途を分ける。古い `relink_to_latest_deal.py` 行番号や「1取引だけ」という説明は使わない。現在の契約関連は過去の契約所属の証明ではない。

複数会社・店舗共有・キー欠損・重複・媒体不一致は人の確認へ回す。応募レコード数を人数や一次対応キュー数と言い換えない。1応募が複数求人に関連する場合、求人別の合計を全体ユニーク件数として加算しない。

## 5. 本文・画像・日付の観測ルール

- 更新時刻の厳密さは不要。JSTの観測日を取得する。ただし取得完了日・媒体の生成日・媒体更新日は別に保持し、不明はnullと根拠を返す。古い生成物の再ダウンロードを今日の観測成功と扱わない。
- 本文raw・表示用全文・正規化hash、公開状態の元値、掲載条件、原本行参照を保持する。同じ内容でも正常観測した日を残し、A→B→Aを消さない。
- HR CSVは `画像1`〜`画像3` に画像URLがあり、限定実データで取得できた。URL変更・掲載順変更・原本hash変更・未取得を別々に判定する。
- **本文差分検知後に画像を取りに行かない。毎回の観測で現在画像bytesを確保する。** 同じURLで中身が差し替わる場合がある。PendingにはURLだけを残さず、失効前に取得したbytesか検証済み耐久保存参照を持たせる。
- 過去URLを今日取得しても過去画像とは扱わない。取得失敗を画像なし・削除・変更なしに変換しない。日次観測の間に公開・削除された未取得画像の全捕捉は保証できない。
- AirWorkの確認済みXLSXには画像枚数のみで画像URL/bytesがない。公開詳細ページ等からの取得は別の調査・実装が必要。非公開求人を公開ページだけで網羅できると約束しない。
- CSVに求人がないことだけで削除確定にしない。対象範囲・生成鮮度・全件取得成功・元媒体の削除仕様を照合する。
- 応募日欠損を作成日で埋めない。`hs_appointment_start=応募日T00:00:00Z` は合成で、実時刻や同日の版境界の証明ではない。確定・日付仮対応・不明を分ける。

## 6. Drive保存・HubSpot参照・アプリ配信

原本は不変保存。内容SHA-256で原本blobを重複排除し、観測・求人・slotの対応は別に残す。manifestには `listingId`、`companyIds`、`observedAt`、`operationId`、画像の `slot/fileId/sha256/mimeType/size` を持たせる。次版には `previousManifestFileId` と `previousManifestSha256` を対で付ける。

HubSpot pointerは次の形。画像bytesや大量の本文JSONをこのプロパティへ詰めない。

```json
{"fileId":"<不変manifestのID>","sha256":"<原bytesのSHA-256>","observedAt":"<RFC3339>"}
```

保存は既存gws user OAuth、アプリの読み戻しはSA readonlyという分担で実証した。無人アップロードの認証・権限は未確定なので、SA readonlyをそのままwriterと扱わない。

CREATE前に予約IDをreceiptへ記録し、同じreceiptで再実行する。HubSpot登録は保存原本照合→専用pointer更新→読み戻し確認。同じ処理の再実行は `already_linked`、過去chainは `already_linked_historical` で区別する。別receiptへの切替や削除後の重複防止は保証しない。

複数writerのCAS/leaseは未実装。全件運用では単一writerまたは排他を設計し、外部書込失敗を永続Pendingとして復旧する。新DBを慣例だけで追加しない。既存storeへの追加を採用する場合もADRに記録する。HubSpotがCRM正本、Driveは不変ファイル保存先とする。

保存先は現在の個人所有実験フォルダを継続し、将来移行する方針。`folderId` は物理保存先、`identityFolderId` は固定の操作識別値。同じファイルIDで移動する場合、HubSpot参照のID差替えは不要。中身を別フォルダへ移す場合は `folderId` を変更、固定identityとreceiptは保持。コピー・再アップロードでIDが変わる移行は別手順。共有ドライブ移行は未実施。

## 7. 全件化で見直す制限

| 現行制限 | 場所／注意 |
| --- | --- |
| 固定36求人スナップショット | 対象選定と手動配置の結果。全件台帳ではない |
| bundle 1〜59求人、32MiB | Rust `job_copy_live.rs::validate_moc`／raw読取、React `mediaCaptureParser.ts`、手動Import表示。元収集側のbuilderにも59制限 |
| 1求人の履歴10件＋現在版 | Rust/React parserと日付比較集計。日次観測を長期運用すると不足する |
| 1求人3画像 | HR CSVの3slot契約、Rust/React parser。Drive manifestの最大100slotと同じ制限ではない |
| 関連API最大20ページ、1社Deal最大100 | `JobReadService`。打切りを全件完了・0件と扱わない |
| 会社別求人1ページ20件 | live APIの `next_offset` で継続。顧客も `next_after` で継続 |
| 選択画像解決cache最大59・30秒 | 求人数上限ではなくプロセス内cache容量。全件化のため無制限化しない |
| 限定収集ツール最大20件、候補選定default8件 | 元gurnardの収集・選定ツール。全件collectorではない |
| Drive復旧候補列挙最大10ページ×100件 | `job_copy_drive_recover.py`。全件に増える場合は上限と完全性の扱いを設計 |

**推奨する追加設計（未実装）：** Driveに全対象の軽量索引と会社/バッチ単位の不変本文・集計ファイルを置き、Rustから認可付き一覧ページ＋選択求人詳細を返す。本文・全履歴・全画像を巨大JSONで毎回返さない。React一覧もページ取得／追加読取にし、画像遅延方式を維持する。既存MOC契約は小さなレビュー用として保持し、新しい全件契約を別バージョンで検証する。索引をCRMマスターDBの代用にしない。

画像cacheは現在1つのmutex保持中に解決するため、異なる求人の同時表示で直列待ちがあり得る。負荷測定後に求人別singleflightと有界並列を検討する。TTL/cacheで会社関連・ログイン認可・原本hash検証を省略しない。

## 8. 実装・ツール・非公開データの所在

mainにある実装：

- React：`frontend/src/screens/job-copy/`、エントリ `frontend/src/entries/job-copy.tsx`。機能タブ、本文/画像、日付履歴/差分、応募属性/理由、課金、市場、逆検索、A/B、報告。
- Rust：`src/handlers/job_copy_live.rs` と同名ディレクトリ、`job_copy_capture.rs`、`job_copy_drive.rs`、`job_copy_image_bridge.rs` と関連ディレクトリ、`job_copy_market.rs`、`src/job_copy_date.rs`。
- Drive保存：`scripts/job_copy_archive_batch.py`、`job_copy_drive_pilot.py`、`job_copy_drive_manifest.py`、`job_copy_drive_snapshot.py`、`job_copy_drive_recover.py`。
- 課金取込：`scripts/job_copy_attach_hrh_performance.py`。HR8桁IDと週/月期間で結合、欠測null、重複/期間重複/不正数値を拒否。媒体応募とHubSpot全期間応募を混ぜない。
- operator連携：`examples/job_copy_drive_link.rs`。`--apply` は外部書込みなので実行範囲を確認する。

元gurnardだけにある未統合ツール・文書も重要：`scripts/job_copy_capture.py`、`job_copy_extract.py`、`job_copy_build_live_capture.py`、`job_copy_discover_applicants.py`、`job_copy_select_applicant_review.py`、`job_copy_build_applicant_review.py`、関連tests、および `docs/architecture/job-copy-sync-current-contract.md`、`job-copy-data-preparation-request.md`、`job-copy-hrhacker-image-verdict.md`。新mainに存在すると仮定しない。依存を確認し必要なファイルだけレビューして取り込む。

元gurnardの非公開データ：`data/job-copy-local/applicant-review/expansion/` のDrive receipt・設定・integration、`reaction-integration/server-settings-320.env`、`integration/drive-moc.json`・`link-results.json`、`data/job-copy-local/live-capture/` の原本/取得証跡。これらは入力例・検証証拠であり本番の自動収集台帳ではない。非公開設定は最新のID/SHAを照合し、古い317応募版を戻さない。

現在のサーバー設定名：`HUBSPOT_ACCESS_TOKEN`、`GOOGLE_SA_KEY_B64`、既存Google OIDC設定、`JOB_COPY_ALLOWED_EMAILS`、`JOB_COPY_MOC_DRIVE_FILE_ID`、`JOB_COPY_MOC_DRIVE_SHA256`、`JOB_COPY_DRIVE_LISTINGS`、`JOB_COPY_MOC_PATH`。値をチャット・Git・frontend env・公開Actions artifactへ出さない。現在の閲覧allowlistは限定運用で、全コンサルタントが閲覧できる設定とは限らない。

## 9. 引継ぎ先の実行順序と返却物

### AirWorkの観測対象・頻度の変更方針（2026-10-07ユーザー指示）

応募をトリガーにした求人取得だけでは、応募前・応募のない求人の本文/画像差分を観測できない。既存応募同期は維持し、**全管理対象アカウントの全求人を、応募の有無と独立した定期観測へ接続する**。公開/非公開/終了の対象台帳を保持し、公開画像取得不能の求人も除外せず状態を記録する。

頻度の数値はまだ未確定。実装の初期目標として全対象を少なくとも1日1回観測し、全巡回の所要時間・媒体制限を測定して短縮可能な間隔を決める。この初期目標は採用済みスケジュールや達成済みSLAではない。「1日3回起動」だけでは全口座を1日3回観測したことにならない。口座/求人ごとに最終成功日時、次回予定、未観測時間、当日対象数/成功数/失敗数/未実行数を保持して実間隔を確認する。

新規求人を発見するため全口座の求人一覧を定期更新し、把握済み公開求人の画像は一覧ダウンロード待ちと切り離して定期取得できる構造にする。本文と画像の取得時点は別々に記録する。本文・参照URLが同じでも画像bytesを観測し、成功した前回原本と比較する。前回/今回観測の間で変化したと報告し、正確な更新時刻は推定しない。

有限並列・単一writer・失敗再試行を用い、応募の多い口座だけで観測枠を使い切らない。生成済みの古いXLSXを今日取得しても今日の本文観測成功にしない。頻度だけ増やして同じ古いファイルを保存する実装を反例として検証する。日次観測の間に変更と復元が起きた場合など、未観測の変化は検知保証の対象外。

1. **棚卸し**：対象を「媒体の全掲載」か「HubSpotの全管理求人」か明示する。基本は管理対象の全取引先についてHR/AW・公開/非公開/終了を含む台帳を作り、アプリの一覧では掲載中を絞れるようにする。媒体全件とHubSpot登録件数が一致すると仮定しない。件数、未対応キー、曖昧関連、口座未観測を返す。
2. **最新契約確認**：HubSpot同期の最新main、配備、既存バッチ/アカウント巡回、書込み担当を確認する。03返却資料ではHRは1日3回全件CSV、AirWorkは巡回で全アカウント日次ではない。現在も同じか再確認する。
3. **既存取得への接続**：媒体ダウンロードを重複起動せず、完了フックから非公開原本・行数・hash・生成日・取得日・品質結果を受け渡す。Actions runner上の一時CSVやcacheだけを長期履歴保存と扱わない。
4. **観測と画像確保**：画像を本文差分と独立に毎回取得。有限並列/429 retry/timeoutを設計し、全件の成功・欠測・失敗を返す。AirWork未対応も台帳から消さない。
5. **不変保存・登録**：原本→manifest→HubSpot pointer→読み戻し。永続Pending、単一writer、停止後の復旧、同じ処理のCREATE0再実行を確認する。
6. **全件読取API/UI**：索引・ページング・選択詳細・長期履歴の契約を設計して段階実装。取引先/媒体/状態/応募数のフィルタ、取得範囲・取得日・欠測を維持する。
7. **自動反映**：成功した不変索引の公開参照だけを切り替える。失敗時は最後の成功を日付付きで維持し、当日取得完了と誤表示しない。旧索引への戻し方を文書化する。
8. **統合・検証・報告**：既存窓口へ必要ファイルとplan/ADR/契約/テスト/非公開データ配置を返す。同じRust/ReactアプリでGoogleログインから確認する。

完成時に返してほしいもの：対象範囲と件数照合表、データ契約、収集/保存/更新手順、失敗・復旧手順、変更ファイル、テスト結果、残る制約、設定項目、画面URL。顧客原本・個人情報・秘密値を報告書に添付しない。

## 10. 完成判定と報告前の反例確認

- 元対象件数＝対応済み＋未解決＋除外（理由付き）をキーで照合。全ページ完了、cursor loop、途中失敗、空ページ、重複、権限不足を検証し、成功サンプルだけで全件完了としない。
- 本文の各媒体列・順序・改行・長文を原本と比較。画像slotとSHA、JPEG/PNG/WEBP、サイズ/MIMEを実ファイルで検証。EChartsは具体的な集計値と初期化完了を確認する。
- 同URLの内容差替え、URLだけ変更、順序変更、本文だけ変更、A→B→A、画像取得失敗、媒体削除後の保存画像表示、bytes確保後URL失効、未確保URL失効を試す。
- 曖昧求人キー・複数関連・応募日欠損・古いCSV・合成時刻・取得不完全は「不明/未取得」のまま。A/Bは別求人IDを保持して明示選択し、自動同一化や変更の因果効果の断定をしない。
- 停止/再起動/途中失敗/再実行でCREATE重複を防止。snapshot更新中の古い画像hash要求が別画像へ化けないこと、古い応答が手動取込を上書きしないことを確認する。
- 未ログイン、期限付き外部パスワード、閲覧対象外メール、無関係会社/求人、任意Drive ID要求で拒否されることを確認。全コンサルタント対応は認可を削ることではない。
- Rust・frontend静的/単体・統合・E2EとPC/mobile監査。全件で初期一覧、選択詳細、画像初回/再試行、同時利用、外部GET数・payloadを実測し、ローカルfixtureを本番Googleログイン成功と報告しない。

直前PR75の確認結果：Rust4119成功/51ignored、frontend82ファイル1018成功、CI E2E39成功/1skip。追加ローカル求人191単体・25E2E、必須CI6すべて成功。これらは現行限定版の回帰結果であり、次の全件化の成功証拠に流用しない。

## 参照文書

- [アプリ組み込み](job-copy-app-integration-handoff.md)：旧件数・旧作業場所の記述は本書と最新mainで補正する。
- [Drive保存と移行契約](job-copy-drive-storage-contract.md)、[拡張結果](job-copy-drive-expansion-results.md)
- [応募理由・課金・市場の契約](job-copy-reactions-performance-handoff.md)、[追加実装結果](job-copy-reactions-performance-results.md)
- [本文先行・画像遅延](job-copy-loading-performance-plan.md)、[読み込み復帰](job-copy-loading-recovery-plan.md)
- 元gurnardの `job-copy-data-preparation-request.md` とHubSpot `claudedocs/job_copy_return/01_*`・`02_*`・`03_*`。01の複数契約への関連変更を、古い同期監査より優先して確認する。

## 11. AirWork実データ取得の更新（2026-10-07）

[実データ取得結果](job-copy-airwork-real-data-results.md)を追加した。既存fetcherで1口座の本文53件を再取得し、原本のファイル生成日を確認した。HubSpotの未アーカイブListingのAirWork ID検索は7,591レコード・404口座で最後まで完了。媒体全求人の網羅とは区別する。

3バッチの143求人・70採用サイトを観測し、24求人から画像51参照・44原本を保存、SHA・サイズ・形式を照合した。118求人は公開ページ取得不可、1求人は構造未対応。本文・原本・欠測・探索範囲の証拠を非公開保存しており、HubSpot/Drive書込みとオンライン画面への反映はまだ行っていない。口座対応が曖昧な495レコードを保留し、全口座CSV取得・Company/Listing関連確認・Drive保存・pointer登録・画面反映へ渡す。
## 12. 顧客全アカウント収集への修正

単口座・求人URLサンプルの取得で止めた不足を修正し、[全アカウント取得](job-copy-all-account-acquisition.md)を開始した。AirWorkは既存台帳の企業資格情報あり561行から重複を除いた519口座を巡回。HubSpot検索に現れた404口座を全顧客数と扱わない。資格情報欠損、クローズ、状態未指定は別記録とする。CSV4口座並列と、取得済み口座の画像2口座並列で、全取得求人の画像を小分けで観測する。全体・各工程の実測時間も記録する。実進捗は非公開summary.jsonを参照し、巡回中に完了と報告しない。

[両媒体の取得契約](job-copy-media-acquisition-handoff.md)に、正本の取得関数、最新の全口座巡回結果、画像保存、失敗・再開・計測の手順を集約した。
