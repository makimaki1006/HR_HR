# 求人文面管理の既存アプリ組み込み

2026-10-05。統合窓口向け。求人・応募同期の所有者は別セッションのまま維持する。

## React移行作業への合流指示

ユーザーの追加指示: 求人文面管理を既存Rustアプリ／React移行へ合流させ、画面はReactにする。画面の正本は `frontend/src/screens/job-copy/JobCopyScreen.tsx`、エントリは `frontend/src/entries/job-copy.tsx`、URLは `/app/job-copy`。Pythonプレビューを本番UIとして採用しない。Rustは認証・認可・HubSpot／Drive取得を担当する。

合流元は `C:/Users/fuji1/orca/workspaces/HR_HR/gurnard`、ブランチ `makimaki1006/edit-app-changes`。統合候補は最新mainの `da81c557` を基点に隔離した `../job-copy-react-join`（`C:/Users/fuji1/orca/workspaces/HR_HR/job-copy-react-join`）、ブランチ `makimaki1006/job-copy-react-join` に作成した。合流元の未追跡ファイルや他作業の差分を丸ごと移さず、求人文面管理の必要ファイル・hunkだけを取り込んでいる。共有checkoutのrebase／checkoutや別の統合窓口は作らない。

候補は最新mainの共通App Shellを保持し、既存ナビに `/app/job-copy` への「求人文面（MOC）」導線を追加した。共通API clientは最新mainの実装をそのまま利用し、変更していない。既存画面の共通部品を二重に作らず、架電CRMや旧求人生成画面の導線・試用条件を保持する。

最小の依存範囲は次のとおり。下の実装表だけをコピーして依存モジュールを落とさない。

- React: `frontend/src/screens/job-copy/` 全体、`frontend/src/entries/job-copy.tsx`、既存共通API clientの利用、既存inputを保持したViteの `job-copy` 追加登録。
- Rust: `src/handlers/job_copy_live.rs` と同名ディレクトリ、`job_copy_drive.rs` と同名ディレクトリ、`job_copy_image_bridge.rs` と同名ディレクトリ、`job_copy_capture.rs`、`src/job_copy_date.rs` と同名ディレクトリ。
- 登録: `src/handlers/mod.rs` の上記モジュール、`src/lib.rs` のdateモジュールと求人API merge、`src/handlers/spa_shell.rs` の画面登録。最新mainに既にある登録を二重追加しない。
- 配置・検証: `.env.example` への求人管理用設定追加、`.dockerignore`／`.gitignore` の `data/job-copy-local/` 除外、`src/router_startup_test.rs` の関連テスト、`tests/e2e/job_copy*.spec.ts` と対応config、Drive保存用scriptsとこの引継ぎ文書。

候補には顧客JSON・画像・資格情報をGit管理対象として取り込んでいない。最新mainは既にViteのビルド・配信段を持つため、求人管理用のDockerfile編集は不要。既存のPDF用Node／Chromium構成も保持する。

受け入れ条件は、最新main上でRust／Reactの関連チェックと実データ契約を通し、既存App Shellから本文・画像・差分・応募構成のReact画面へ到達できること。本番反映後は既存GoogleログインからのAPI認可・画像表示を確認する。ログ・秘密値・顧客データをコミットしない。日次求人同期の改善は別所有者へ残す。

合流依頼は既存のReact移行担当セッション（同じgurnardのClaudeセッション）へOrcaのメッセージで送信済み。正しい依頼の受付IDは `msg_562464581dea`。受付結果と本文は非公開 `app-integration/react-join-receipt-corrected.json`／`react-join-message.txt` に保存。送信受付を担当者の受領・作業開始・本番反映と同一視しない。担当者の入力中draftを保持し、別の統合窓口は作成していない。

## 取り込む動作

既存ダッシュボードの「求人文面（MOC）」から `/app/job-copy` のReact画面を開く。専用ViteエントリとRustのシェル／API登録は既にある。今回、ローカルにしかなかったレビュー用JSONを現在の非公開Driveへ配置し、既存サービスアカウントでサーバーから取得する経路を追加した。顧客JSONや画像をGit・Dockerイメージへ埋め込まない。

画面は取得時点の36求人・応募317件を表示する。画像のある28求人・45参照はHubSpotの専用プロパティから履歴manifestを確認し、Drive原本のアプリ内URLへ解決する。求人本文・応募の正本や既存同期処理を置き換えない。

## サーバー設定

非公開の配置済み設定は `data/job-copy-local/applicant-review/expansion/app-integration/server-settings.env`。秘密値は含まず、次の項目を持つ。

| 設定 | 役割 |
| --- | --- |
| `JOB_COPY_MOC_DRIVE_FILE_ID` | 今回の不変レビュー用JSONのDrive ID |
| `JOB_COPY_MOC_DRIVE_SHA256` | JSON原bytesのSHA-256。IDと必ず両方設定する |
| `JOB_COPY_DRIVE_LISTINGS` | 画像経路を有効にする28求人のallowlist |
| `JOB_COPY_ALLOWED_EMAILS` | 閲覧可能なメール一覧。現在は指定された1アカウント |
| `JOB_COPY_MOC_PATH` | 今回は空。ローカルファイル指定へ依存しない |

別途、既存の `HUBSPOT_ACCESS_TOKEN`、`GOOGLE_SA_KEY_B64`、Google Workspace OIDCのサーバー設定が必要。サービスアカウントは現在のDriveフォルダにreader権限を持つ。秘密値をフロントエンド環境変数へ設定しない。

リモートのID/hash設定が片方だけ・不正な形式ならエラー。設定済みDriveファイルの取得・hash・MIME・schema検証に失敗した場合も停止し、ローカルファイルや架空求人へ切り替えない。クラウド画像allowlistの不正値や、現在版に必要な画像対象の未網羅も停止条件とし、設定ミスをdataURI表示への切り替えで隠さない。リモート設定を両方空にした場合に限り、従来のローカルファイル経路を利用できる。今回は `JOB_COPY_MOC_PATH` が空なので、クラウド設定を外すだけでは復旧にならない。設定変更後はアプリを再起動し、復旧時は過去の不変snapshotのID/hashを対で戻し、対応する画像allowlistも保持する。

## 保存・更新手順

operator用Python環境には `python -m pip install Pillow google-auth requests` で依存を準備し、既存の `gws` CLIとuser OAuth認証を用いる。SA鍵は読み戻し用で、アップロード用認証とは別。これらは運用者が保存ツールを実行する環境の依存であり、アプリのDockerイメージへPythonを追加する要件ではない。

レビュー用スナップショットの不変保存は次のoperatorコマンド。原JSON bytesを保持し、予約IDを先にreceiptへ記録してからCREATEする。同じreceiptを維持すれば再実行時は同じIDを再利用し、別identityへの誤変更は予約前に停止する。共有権限やHubSpotレコードはこのコマンドから変更しない。

```powershell
python scripts/job_copy_drive_snapshot.py --input <レビューJSON> --storage-config <既存Drive設定JSON> --service-key <SA鍵ファイル> --receipt-dir <非公開receiptディレクトリ> --output <非公開結果JSON> --upload
```

次の取得済みレビューへ更新する際は新しい不変ファイルを保存し、サーバーのID/hashを対で切り替える。現在のスナップショットに本番の日次同期は接続していない。古いファイルを上書きする方式は採らない。

## 実装の場所

| 場所 | 変更 |
| --- | --- |
| `src/handlers/job_copy_drive.rs` | 設定済みJSONのreadonly取得、32MiB・MIME・hash検証 |
| `src/handlers/job_copy_live.rs` | 認証後のクラウドJSON取得とschema検証、画像参照を最大4求人並行で解決 |
| `src/handlers/job_copy_image_bridge.rs` | 並行解決のため既存clientを共有するClone実装 |
| `scripts/job_copy_drive_snapshot.py` | 不変JSONの予約ID保存・CREATE・読み戻し検証 |
| `scripts/job_copy_drive_manifest.py` | readback上限を用途別に指定可能にした。既定5MiBは維持 |
| `frontend/src/screens/job-copy/SnapshotErrorNotice.tsx` | 認証・権限・設定・取得失敗ごとの案内 |
| `frontend/src/api/client.ts` | 最新mainの共通実装をそのまま利用。候補では変更なし |
| `templates/dashboard_inline.html` | 求人文面リンクの説明を実データ閲覧へ更新 |
| `.env.example` / `.dockerignore` | サーバー設定契約、非公開データのビルド除外 |

画像参照の置換は現在版の全対象の検証成功後にまとめて反映する。1件の失敗を部分的な成功として返さない。今回検証したDrive画像は現在版の45参照で、過去版の画像原本は0件。過去画像を観測・保存できたとは扱わず、存在しない過去画像を生成・補完しない。既存の画像APIはprevious manifest chainを検証できるが、レビューJSONの過去版画像を対応する過去manifestへ自動解決する処理は未実装。今後はレビュー取得側にも履歴解決を接続する必要がある。現在版のallowlist網羅検査は、全履歴画像のクラウド保存を証明しない。

## 検証と残る条件

以下の検証結果は合流元で取得した証拠であり、隔離候補の全チェックや本番配備の完了を意味しない。候補の検証結果は統合窓口で別途確定する。

Rust関連55件、Drive関連54件、React関連単体・型チェック・変更箇所Lint・Vite buildが成功。full `build_app`の未認証画面遷移とAPIの401／no-storeも検証した。最新画面の成功E2E2件で45画像の表示と原本hash一致、失敗応答E2E6件で案内と架空データへ切り替えないことを確認した。失敗E2Eは失敗応答だけを差し替えている。

`cargo build --bin rust_dashboard`、`cargo clippy --lib`、変更したRustファイルのrustfmt確認も成功した。Clippyにはリポジトリ全体で378警告が残るため、警告ゼロとは扱わない。実APIのレビュー取得は17,970msで、今回の1回の検証では画面の30秒上限内だった。

実API検証では配置済みJSONをサービスアカウントで取得し、テスト内で作ったOIDCセッションをCookieとしてAPI Routerへ渡した。36求人・317応募・45画像参照を取得し、前の検証済みレビューとJSON全体が一致した。これは実Drive／HubSpotの読み取り成功を含むが、Googleへのログイン操作や配備済み本番の認証を実証したものではない。

統合窓口でコードと環境変数を取り込み、既存アプリを配備してGoogleログインからの確認を行うこと。ローカルの確認画面をオンライン本番として案内しない。日次収集・永続Pending運用・複数writer制御、応募の履歴対応314件未確定、画像1枚ごとの約3秒の待ち時間は残っている。

検証証拠・配置済み設定はGit管理外の `data/job-copy-local/applicant-review/expansion/app-integration/`。保存先を移行する際は[既存の移行契約](job-copy-drive-storage-contract.md)に従い、読み取り権限を確認する。
