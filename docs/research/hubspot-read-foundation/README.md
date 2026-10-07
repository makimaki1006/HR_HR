# Headless CRM — HubSpot準拠の読み取り基盤とUI

リポジトリへの反映は一つの連携窓口に集約する。変更ファイル、取り込み手順、設定、検証状態と完了条件は[連携窓口向け引き継ぎ](../../architecture/headless-crm-integration-plan.md)を正本とする。現在は本番未反映で、認証後の実アプリ画面からのAPI成功経路は未検証。

> 2026-10-08 追記: `/app/crm` を `?view=` なしで開くと、HubSpot の実データを読む架電画面(架電キュー)が開く。以下の節で `/app/crm` と書いている MOC・「HubSpot連携・速度確認」・架空12案件の連続架電画面は、いまは `/app/crm?view=moc`(Vite では `crm-preview.html?view=moc`)で開く。見本は `?view=reference`、単発の架電 MOC は `?view=single`。

## Rust経由の定義API連携（2026-10-01）

`/app/crm?view=moc` の「HubSpot連携・速度確認」を開き、「HubSpot定義を取得」で実定義を読み取る。初回アクセスだけでは外部APIを呼ばない。Contact/Companyの項目名とDeal入力の項目名・型・選択肢をAPIの定義へ切り替え、パイプライン/ステージは参照用に表示する。顧客値は引き続き架空。他の「1件」「基準」画面は固定デモ。取得失敗時はエラーと現在使っている定義の状態を明示する。

サーバー設定: `HUBSPOT_ACCESS_TOKEN` と、カンマ区切りの `CRM_METADATA_ALLOWED_EMAILS` をRust側に設定し再起動する。今回の許可メールはユーザー指定の `s_fujimaki@f-a-c.co.jp`。ローカルのgit管理外 `.env` に既存のHubSpot資格情報とこの許可メールを設定済み。Render等の本番環境への反映は未実施。Google OIDC本人確認が必須で、共有・期限付きパスワードでは403。未ログインはJSON 401、設定不足は503。空の許可リストでは全員拒否する。CRM全体のRBACや顧客閲覧権限を決定したものではない。

速度表示は、ブラウザ往復・Rust処理・HubSpot取得を分ける。Rustは定義3種とパイプラインの4GETを並列実行し、成功した結果を60秒メモリキャッシュする。キャッシュ時はHubSpot通信なしで0msを返し、取得日時は元の値を保つ。再取得ボタンはキャッシュを使わない。HTTP 429/5xxでは最大1回、2秒以下の待機で再試行する。各リクエストの上限は15秒で、2試行なら約32秒までかかりうる。失敗を成功や空データへ置き換えず、上流エラー本文はブラウザへ渡さない。

同じRustアダプタの速度確認: `cargo run --example crm_metadata_benchmark --offline -- .env`。初回・キャッシュ・明示再取得の3サンプルを出力する。`main()` やDB初期化を呼ばず、GETのみを実行する。トークンや項目名・実レコード値は計測結果に出力しない。ローカルの単発測定であり、本番のブラウザ表示時間・負荷試験・p95を意味しない。

実測（2026-10-01 14:21 UTC、ローカルRustアダプタ、[3サンプル](metadata-benchmark.jsonl)）: レビュー対象42プロパティ、14パイプライン、190ステージの取得成功。

| 条件 | HubSpot取得 | Rust取得処理全体 |
|---|---:|---:|
| 初回 | 1,016.6ms | 1,018.8ms |
| 60秒キャッシュ | 通信なし | 0.106ms |
| 明示再取得 | 925.3ms | 927.2ms |

Rust取得処理全体はキャッシュ待機・外部GET・定義変換を含む。HTTPルートの認可・JSON送信・ブラウザ通信/描画を含まない。ローカル `.env` にはGoogle OIDC設定がないため、本人のGoogleログインを経由したブラウザの実API成功経路は未実測。OIDC設定済みのアプリで、同じ許可メールを設定してから画面の速度表示を確認する。共有パスワードを代替認証にしない。

検証: Rustの定義連携7件、画面配信10件、ルーター構築2件、ts-rs生成1件が成功。Reactは33テスト（7ファイル）、typecheck・lint・buildが成功。実APIは読み取りのみ成功を確認。HTTPモックで選択肢の内部値保持、順序、同時取得の集約、キャッシュ、明示再取得、認証拒否、上流HTTPエラー/不正JSONの秘匿化と復旧を検証した。[速度確認パネル](metadata-panel-preview.png)。

公式仕様: [Properties API](https://developers.hubspot.com/docs/api-reference/legacy/crm/properties/guide)、[Pipelines API](https://developers.hubspot.com/docs/api-reference/legacy/crm/pipelines/guide)。数値バージョンの既存GETを使用し、IDと表示ラベルを分離して保持する。

## 実プロパティ対応MOC（2026-10-01）

Reactの既存 `/app/crm` に組み込んだ架空12案件(いまは `/app/crm?view=moc`)の連続架電画面。実HubSpotのDealプロパティ23件に合わせて、ラベル・型・選択肢の内部値を実装した。上部の絞り込み・集計は初期状態で折りたたみ、一覧をスクロールして操作できる。「＋ 詳細・入力」で採用ヒアリング・商談・停止理由の項目を展開する。会社基本情報は共有し、案件の入力は仮のDeal IDごとに分離する。

次回架電の日付と時間は別項目で、時間は実定義の45選択肢。複数選択・既存の未知値も保持する。連携元の項目は参照専用。架電停止の理由または停止希望の選択で発信デモを無効化する。記録後は絞り込み内の未処理・番号あり・停止していない対象へ進む。入力の必須条件はMOCの暫定案で、実運用での確定が必要。

実顧客の読み取り・HubSpot保存・Zoom Phoneは未接続。下書きとデモ記録はメモリ内のみで再読み込みすると消える。資格情報はブラウザへ渡していない。CRM専用OIDC/RBAC、書き込み認可、Durable Retryは後続工程。現在のRust画面入口は既存のログイン保護を使用する。

検証: フロントエンドのtypecheck・lint・30テスト（6ファイル）・build成功。Rustの `handlers::spa_shell::tests` 10テストと対象ファイルのrustfmt成功。Orcaブラウザで同一会社の別案件への入力分離、募集職種・人数・課題感の複数選択、次回日時付きデモ記録、次対象への移動、停止対象の発信無効化を確認した。自動E2Eと実アカウントの値の照合は未実施。

[MOC画面](moc-preview.png) · [ブラウザ検証記録](moc-call-record-verification.txt) · [実装計画](../../architecture/headless-crm-integration-plan.md)

ローカル確認: Vite起動中なら `http://127.0.0.1:5173/static/app/crm-preview.html?view=moc`。Rustアプリでは `frontend/` で `npm run build` 後、サーバーを再起動してログインし `/app/crm?view=moc` を開く（manifestは起動時に読み込む。`?view=` なしの `/app/crm` は架電画面）。本番へはデプロイしていない。

調査日: 2026-10-01。段階: 公式標準UIを参照したサンプル版。実アカウントの画面・プロパティ・権限は未確認。

追記: ユーザー指定のHubSpotフォルダと既存資格情報を参照し、実プロパティ定義の読み取りを確認した。[接続・実プロパティの確認結果](account-property-findings.md)。現行UIは引き続き架空データのデモで、BPOの権限・必須条件・Workflow・実レコード表示は未確認。

次の実装は [実プロパティ対応とアプリ組み込み計画](../../architecture/headless-crm-integration-plan.md)を参照。実定義の照合、認可付き読み取り、入力、耐久保存、Zoom連携の順で進める。

## 複数顧客の連続架電版（2026-10-01）

一覧を優先する表示へ改善。編集前の方針は「顧客のクリック箇所を枠付きボタンにする / 上部の説明・集計・検索条件を初期状態で折り畳む / 一覧を画面の残り高さへ広げ、行の余白を詰める」。上部を開閉してもフィルタと下書きを保持する。折り畳み時にも表示件数・記録件数・絞り込み中の表示を残す。デモであることはヘッダーとフッターに残し、通話中の情報は一覧上部に常時表示する。

省スペース版の検証: typecheck / lint / 26テスト / build成功。ローカルブラウザーで一覧領域の高さ705px・先頭行129pxを実測。上部展開時は一覧504px、折り畳みで705pxへ戻ることを確認。ページのscrollHeightとclientHeightはともに870pxで、ページ全体の余分なスクロールを解消した。顧客欄の「＋ 詳細・入力」ボタンは枠・背景・hover・展開状態で操作箇所を示す。[一覧優先の画面](batch-compact-preview.png)。

顧客名ボタンに詳細プロパティの開閉を追加。担当者・会社の既存プロパティ一覧と、職種・採用予定人数・開始時期・課題のヒアリング入力を展開する。入力できるプロパティは限定し、オーナー・ステータスなどは参照のみ。折り畳みでも下書きを保持し、会社単位の情報は同じ会社の別担当者にも反映する。実HubSpot更新は未実装。

詳細入力の検証: typecheck / lint / 26テスト / build成功。ブラウザーで展開・折り畳み、再展開時に「3人」「11月上旬」「採用責任者」が残ることを確認。同じ会社の小林さんには人数・時期が反映され、個人の役職は「拠点責任者」のままであることを確認。[詳細の画面](batch-property-preview.png)、[入力保持・共有の検証](batch-property-verification.txt)。

追加改善: 架電先を12件の架空データへ増やし、一覧内スクロールと固定列見出しを追加。会社・担当者の検索、担当オーナー、対応状態、電話番号の有無によるAND絞り込みを実装した。通話中は絞り込みを無効化し、非表示の顧客の下書きも保持する。次の対象は絞り込み内で選ぶ。

[入力要件の暫定案](calling-input-requirements.md)に必須条件・入力用途・HubSpot保存候補と未決事項を記載した。会話相手・関心度・次のアクションを任意の展開入力としてデモに追加。実HubSpotのプロパティ名やWorkflowは確定していない。

改善版の検証: typecheck / lint / 25テスト / build成功。ブラウザーで検索「森」が1件、オーナー「山本 健」＋番号ありで4件、存在しない会社で0件になること、解除後にメモが復元されることを確認。追加3項目がデモ記録に残ることと、一覧内スクロール位置1500pxへの移動も確認。[絞り込み検証](batch-filter-verification.txt)、[入力の検証](batch-input-verification.txt)、[一覧の画面](batch-filter-preview.png)、[追加入力の画面](batch-input-preview.png)。

追加指示「1画面に複数顧客を表示して連続架電」に合わせ、主画面を一覧型へ変更した。各行に顧客・担当者、前回の電話と申し送り、発信、今回の結果とメモを並べる。従来の1件表示は `?view=single`、HubSpot基準版は `?view=reference` から開ける。

編集前の計画: 既存fixture・架電結果の検証・次対象の選択処理を再利用して一覧用Reactコンポーネントを追加する。各行の下書きを独立して保持し、記録後に次の発信ボタンへフォーカスを移す。通話中は他の顧客への発信を無効化する。実接続・バックエンド・永続化は追加しない。

次対象は番号がある未記録・未後回しの顧客から選ぶ。記録済みの行も一覧に残す。次の行へ移っても自動発信はせず、担当者が発信ボタンを押す。詳細履歴と次回日時は必要なときに展開する。再読み込みでデモ状態は消える。

検証: typecheck / lint / フロントエンド24テスト（5ファイル）/ build成功。Orca内ブラウザーで、3顧客の同時表示、顧客別メモ保持、通話中の他顧客発信・記録の無効化、結果未選択エラー、高橋さんの記録後に番号未設定の森さんを飛ばして小林さんの発信ボタンへフォーカスすることを確認した。画面遷移せず、記録済みの行と次の顧客の下書きが残ることも確認。[一覧型の画面](batch-call-preview.png)、[通話中の検証](batch-call-active-verification.txt)、[次対象への移動](batch-call-next-verification.txt)。Rustは今回変更していない。

## 架電特化版（2026-10-01）

追加指示「電話する事だけに特化」に従い、既存Reactプレビューの主画面を架電ワークスペースへ変更。基準版は `?view=reference` で比較用に残す。

今回の計画: 架電先一覧 / 電話・前回履歴 / 結果入力の3領域へ絞る。通話デモを提供し、通話中の架電先切り替えを止める。下書きはレコード単位の画面メモリに保持する。結果の選択を必須にし、再架電の約束では日時も必須にする。電話番号未設定のレコードは発信デモ・結果記録を無効化し、後回しにできる。実HubSpot・Zoomへの書き込みは追加しない。

デモの進行: 発信デモ → 終了 → 結果 / メモ / 次回日時 → デモ記録して次へ。完了・後回しを除いて次の対象へ進む。全件を確認したら終了画面を表示する。「デモ記録済み」はこの画面内の状態で、HubSpot保存済みやPending Syncを意味しない。再読み込みでは下書き・デモ記録とも消える。

実接続時は、Zoomイベントから通話状態・時間を取得し、HubSpotの実際の架電結果プロパティへマッピングする。「架電停止」はサーバ側で架電対象から除外する条件と照合する。メモ・結果・次回予定の保存が成立してから次へ進む処理は、Durable Retryと冪等性の設計・権限決定後に追加する。

検証: typecheck / lint / フロントエンド22テスト / build成功。Orca内ブラウザーで、対象切り替え後のメモ復元、通話デモ中の対象・結果・次へボタン無効化、デモ開始・終了、未選択結果と再架電日時のエラー、記録後の次対象への遷移、番号未設定の発信無効化を確認。後回しを含めてリストを最後まで操作し、デモ記録2件・後回し1件で終了することを確認した。[架電特化の画面](call-preview.png)。

## 方針と実装前の確認

ユーザー指示: 最初にHubSpotに準拠したUIを作り、その後、架電担当のBPOアルバイト向けに効率を高める。初期版では3列の情報構造を維持する。HubSpot全機能の複製は目的にしない。

`CLAUDE.md` と architecture 配下の frontend-react-migration / headless-crm-design / architecture-decisions / react-full-migration-plan を照合した。React Phase 0、Google OIDC、ログイン方式のセッション記録は現行コードに存在する。9月29日の作業計画の「Phase 0未実装」「CRM画面は1A完了後」は現状と異なり、正本のADR-014補足を優先する。HubSpotへの直接読み取りアダプタは見当たらず、コンサルKPIはSheets由来の情報と取引Deep Linkを利用している。

既存競合調査は別担当。survey / competitor / dashboard_inline / libのルートには変更を加えず、React既存シェルに画面名を追加する範囲に留める。新DB、CRM書き込み、Zoom連携、権限ポリシーの確定は今回の範囲外。

## 実装計画（編集前に確定）

1. 公式レコード画面の参照画像を保存・確認し、標準構成とAPIの対応を調査する。
2. `frontend/src/screens/crm/` に表示モデル、架空fixture、検索・レコード切り替え・活動フィルタの処理を分けて作る。
3. Reactの3列レコード画面と一覧を作り、Viteエントリと既存Rustシェルの画面名を追加する。旧ナビには追加しない。
4. サンプルであることを常時表示する。実レコードへのリンクは実データのIDからサーバが生成する。fixtureのIDでHubSpotへ誘導しない。
5. TypeScript / lint / unit / build と、利用可能なローカルブラウザーで表示・主操作を検証する。実データ一致は未検証として残す。

## 参照した標準UI

[HubSpot公式: レコード画面の構成](https://knowledge.hubspot.com/records/work-with-records)（2026-09-09更新）。本文と次の公開画像を確認した。画像は公式資料であり、御社のHubSpot画面を撮影したものではない。

| 領域 | 参照画像 | HR_HRの初期版 |
|---|---|---|
| 左 | [基本情報](references/record-properties.png) | 名前、連絡先、担当者、主要プロパティ |
| 中央 | [活動履歴](references/activity-timeline.png) | 概要 / アクティビティ、活動種別・担当者・期間・本文検索、詳細展開 |
| 右 | [関連レコード](references/associations.png) | 関連会社・コンタクト・取引、主たる会社のラベル、関連レコードへ移動 |

公式構成から選んだ表示範囲で、契約に依存するAI機能・Revenue等は再現しない。実アカウント固有のカード順序・カスタムタブ・必須項目は参照URL受領後に照合する。

## 読み取りAPIの対応（提案、未公開）

| 画面に必要なもの | HubSpot読み取り | HR_HR側の設計 |
|---|---|---|
| 基本情報 | `GET /crm/v3/objects/{contacts,companies,deals}/{id}?properties=...` | 取得プロパティをサーバの許可リストで限定。null・欠落・未設定の違いを保持 |
| プロパティ定義 | `GET /crm/v3/properties/{objectType}` | internal nameと表示ラベル・選択肢を分離。画面の日本語からAPI名を推測しない |
| 関連レコード | Associations v4のページ付き読み取り | IDとラベルを取得し、必要な表示プロパティをbatch/readで補完。先頭だけで全件と扱わない |
| 活動履歴 | 関連する calls / notes / tasks / meetings / emails の取得 | オブジェクトごとの時刻・担当・本文を正規化。CRMイベント全体が単一APIで取得できるとは仮定しない |
| 担当者名 | Owners API | owner IDとuser IDを区別し、未解決IDは未解決として表示 |
| 取引ステージ | Pipelines API | pipeline ID / stage IDと表示名を別に保持 |
| 一覧 / 検索 | Objects一覧またはSearch API | ページング必須。SearchのPOSTは読み取り用途。権限範囲をサーバで先に決める |
| HubSpotで開く | 既存portal IDとobject IDで生成 | Contact=`0-1`、Company=`0-2`、Deal=`0-3`。portal IDをUI内でハードコードしない |

公式資料: [Contacts](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/guide)、[Associations](https://developers.hubspot.com/docs/api-reference/latest/crm/associations/associate-records/guide)。他オブジェクトのAPI仕様・最小scope・実アカウントでのアクセス可否は接続実装時に追加検証する。

想定HR_HR API: `GET /api/crm/{contacts,companies,deals}`、`GET /api/crm/{contacts,companies,deals}/{id}`、`GET /api/crm/{objectType}/{id}/activities`。返却はRustのSerialize + ts-rsによる型生成へ移す。今回のTypeScript表示モデルはfixture専用で、API応答をそのまま信用する契約ではない。

関連と活動は `loaded / empty / partial / error / not_requested` を区別する。取得失敗時に空リストで「履歴なし」と表示しない。本文はtextとして表示し、HubSpot由来のHTMLを直接DOMへ挿入しない。

## 実接続の条件

- Google OIDCの本人確認済みセッションをRustで確認する。
- RBACの保持先・役割・他部署レコードの扱いは未決定。既存共有パスワードでCRM全件閲覧を許可しない。サービスキーの権限は利用者の権限と同じではない。
- 初期読み取りscopeと認証方式を決め、トークンはRustの環境変数だけに設定する。ブラウザー・fixture・ログへ出さない。
- `401 / 403 / 404 / 429 / 5xx / timeout` を区別し、読み取りのみ回数上限のあるretryにする。`Retry-After`、並列数、API呼び出し数を管理する。
- 実アカウントでContact / Company / Deal各1件を画面と照合する。関連ラベル、活動の表示範囲、選択肢、日付とtimezoneも確認する。

## BPO特化の次段階

基準版を確認した後に、左を架電キュー、中央を前回履歴と担当者・電話番号、右を常駐Zoom Phoneへ変える。基本情報・活動・関連カードは再利用する。架電結果・メモ・次回日時・保存して次へはDurable Retryと冪等性が成立してから実接続する。

確認したい運用: 当日の架電対象の出どころ、優先順位、前回の結果と次回予定の内部プロパティ名、発信してはいけない条件、通話前に見る項目、結果の選択肢。効率は1件あたりの操作数・履歴確認時間・入力漏れ・二重架電で評価する（具体的な目標値は未設定）。

## 検証記録

ローカルプレビュー: `frontend/` で `npm ci` → `npm exec vite -- --host 127.0.0.1 --port 5173 --strictPort`。ブラウザーで `http://127.0.0.1:5173/static/app/crm-preview.html?view=reference` を開く。Rustアプリで利用する場合は `npm run build` 後にRustサーバーを再起動し、ログインして `/app/crm?view=reference` を開く（manifestは起動時読み込み。`?view=` なしは架電画面）。本番デプロイは実施していない。

- `npm run typecheck`、`npm run lint`、`npm run test`（3ファイル18件）、`npm run build` が成功。
- `rustfmt --check --edition 2021 src/handlers/spa_shell.rs`、`cargo test --lib handlers::spa_shell::tests --offline -j 2`（7件）が成功。Rustの警告は既存コードに84件。
- Orca内ブラウザーで基本情報と活動の表示を確認。コールに絞ると1件、関連会社へ移動するとコンタクト2件、関連取引へ移動すると金額¥300,000が表示されることをDOMで確認した。概要タブの更新日時2026/09/30 14:40と、一覧検索「森」で森大輔1件だけが残ることも確認した。
- [初期画面](crm-preview-desktop.png)、[会社](crm-preview-company.png)、[取引](crm-preview-deal.png)のスクリーンショットを保存。公式参照画像と情報構造を比較した。全ピクセル一致を目的としたクローンではない。
- 実アカウントのUI比較、実HubSpot API読み取り、BPO業務の効率測定、モバイル実機、CRMのPRごとの自動E2Eは未実施。ブラウザー確認は手動のローカル操作検証で、CIのE2Eを代替しない。

プロパティ・活動・関連カードのReact表示を残したまま、fixtureをRustの型付き読み取り応答に置き換える。次の実装はUI参照URLと最小scope・RBAC方針が揃ってから行う。
