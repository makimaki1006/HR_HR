# 求人画像 Drive 連携の統合引継ぎ

2026-10-05。既存 HR_HR の Rust/Axum と React に、HubSpot の求人参照から Drive の保存画像を取得する経路を追加した。実データによる限定検証は **2 求人・原本 3 件**で成功。本番配備・本番ログインを通した画面確認は未実施。

求人・応募の既存同期は別 HubSpot プロジェクト／別セッションの所有。この変更では既存 CSV 同期やメモ転記を変更せず、統合窓口からまとめてレビュー・反映する。commit・deploy は実施していない。

## 実データで確認した範囲

| 原本 | bytes | Rust の取得時間（初回標本） |
| --- | ---: | ---: |
| JPEG 1 | 99,245 | 3,161 ms |
| JPEG 2 | 190,153 | 4,627 ms |
| PNG 1 | 159,403 | 3,725 ms |
| 合計 | **448,801** | — |

原本は JPEG 2 件・PNG 1 件。ユーザー OAuth で保存し、サービスアカウントの Drive readonly による読み戻し、Rust 経由の取得とも SHA-256 を照合した。Rust の実検証結果は求人 2 件とも `linked`、画像 3 件の保存ファイルも原本 hash に一致した。

同じ manifest を Rust CLI で再実行した結果は **2 件とも `already_linked`、画像 3 件を再検証**。再度の pointer 更新を必要としないことを限定範囲で確認した。

取得時間は `ImageBridge::image` の認可・参照解決・原本取得を含む、逐次実行 3 件の初回標本。画像 GET 単体の帯域、同時利用時の throughput、SLA を表す数値ではない。

保存先は [実験フォルダ](https://drive.google.com/drive/folders/1j9UC3GpgmUuDG4TpYU12031UV6mMWDIp)。マイドライブの個人所有の実験領域であり、本番保存先の採用ではない。フォルダへのサービスアカウント reader 付与を確認した。一般公開の画像 URL は使わない。

## HubSpot と manifest の契約

求人 LISTING `0-420` に追加した専用プロパティは **`job_copy_drive_manifest_v1` のみ**。string/text に次の JSON pointer を保持する。実 ID はこの文書へ記載しない。

```json
{"fileId":"<manifestのDrive ID>","sha256":"<manifest原bytesのSHA-256>","observedAt":"<RFC3339観測日時>"}
```

manifest は `schemaVersion:1`、`listingId`、`companyIds`、`observedAt`、`images`、`operationId`。各画像は `slot`、`fileId`、`sha256`、`mimeType`、`size` を持つ。次版には `previousManifestFileId` と `previousManifestSha256` を両方持たせる。company・画像は各最大 100、slot は 1–100、未知のフィールドは拒否する。

`operationId` は指定 folder と、operationId を除いた canonical manifest から計算する。同じ処理は同じローカル receipt 内の予約 Drive ID を使う。pointer の `sha256` は operationId を含む保存済み manifest **原 bytes** の hash であり、operationId 自体とは異なる。

manifest 本体は不変。Drive `appProperties` の `job_copy_kind=manifest` と `job_copy_sync=pending` は探索用で、固定 pending だけでは未連携・完了を確定しない。HubSpot current pointer と previous chain が acknowledgement の根拠となる。再実行は `already_linked`／`already_linked_historical` を区別する。原本確認後に専用 pointer だけ PATCH し、読み戻し一致で `linked` とする。

current の事前読取と previous/date 照合は実装したが、複数 writer に対する compare-and-swap や分散 lease は未実装。検証は単一 operator による逐次実行で行った。scheduler・lease・同時実行安全性を備えた本番 Durable Pending 運用の完成とは扱わない。

## 実装と起動設定

| 場所 | 役割 |
| --- | --- |
| `scripts/job_copy_drive_pilot.py` | 原本の予約 ID 保存・CREATE・読み戻し検証 |
| `scripts/job_copy_drive_manifest.py` | 求人ごとの不変 manifest 保存と検証 |
| `scripts/job_copy_drive_recover.py` | Drive のみから manifest 候補を再構成 |
| `src/handlers/job_copy_drive.rs` | SA 認証、固定ホストの Drive readonly 取得、hash 照合 |
| `src/handlers/job_copy_image_bridge.rs` | 会社→取引→求人の認可、pointer／版 chain、限定 publish |
| `src/handlers/job_copy_live.rs` | 既存アプリ API と MOC の実行時画像 URL 解決 |
| `examples/job_copy_drive_link.rs` | 少数求人の operator 用連携検証 |

既存アプリの環境変数は `HUBSPOT_ACCESS_TOKEN`、`GOOGLE_SA_KEY_B64`、`JOB_COPY_ALLOWED_EMAILS`、`JOB_COPY_MOC_PATH`、`JOB_COPY_DRIVE_LISTINGS`。最後の項目は画像経路を有効にする求人 ID のカンマ区切り allowlist。秘密値はサーバーだけで扱い、ブラウザーへ渡さない。Drive reader は readonly scope を使う。

`/api/job-copy/moc` と `/api/job-copy/image` は既存 Google Workspace OIDC セッションとユーザー allowlist を確認する。画像は company・listing・manifest・slot を入力に認可／chain 解決し、任意の Drive file ID をそのまま取得する proxy にはしない。画像レスポンスは `private, no-store`。これらはコードと限定サービス検証の確認範囲であり、配備後ログインの実証は別途必要。

少数連携 CLI は次の引数順。`--apply` はプロパティ整備・求人 pointer の外部書込を行うため、既存承認範囲を確認して operator が実行する。

```powershell
cargo run --example job_copy_drive_link -- <HubSpotの.envパス> <SA鍵ファイルパス> <manifest-results.json> <非公開output-dir> --apply
```

## Recovery と失敗時の扱い

実 GET で、ローカル upload receipt を読まず Drive だけから **2 manifest・画像参照 3 件**を再構成した。CREATE・HubSpot 書込は 0。出力は manifest-results と同じ `{results:[...]}` 契約で、後段 CLI の照合入力に利用できる。

```powershell
python scripts/job_copy_drive_recover.py --folder-id <実験フォルダID> --readback-service-key <SA鍵ファイルパス> --output data/job-copy-local/drive-experiment/recovered-manifests.json
```

列挙は指定フォルダ直下・JSON・kind=manifest に限定し、100 件／page・最大 10 page。重複 ID、cursor loop、不完全検索、上限超過、hash 不一致は全体失敗とし、前回の完全な出力を置換しない。候補一覧は current/history の HubSpot 照合前に「未連携一覧」と呼ばない。Drive の削除済みファイルや、別 OAuth application に非公開の appProperties は復元保証の対象外。

gws の JSON media GET は成功時でも `--output` が空で JSON が stdout に出る挙動を実証した。manifest の原 bytes 検証は `--readback-service-key` による SA の固定ホスト GET を使う。JPEG と同じ CLI 出力処理で検証しない。GET の一時エラー・認証更新 transport 失敗は最大 3 回の bounded retry。CREATE は盲目的に再送せず、保存済み予約 ID を照合する。

## MOC 表示と残作業

元の `data/job-copy-local/real-moc.json` は画像 20 参照すべて data URI のまま。Rust が有効対象 2 求人の content hash と slot を照合し、実行時 hydration で画像 3 件の URL を `/api/job-copy/image` へ解決する。元 JSON をオンライン画像へ書き換えていない。取得証拠がない過去画像は補完せず、今回 manifest と異なる過去画像は既存 data URI のまま残る。

非公開の `integration/drive-moc.json` と Rust 検証済み画像ファイルを使う loopback preview は、画面レビュー用のローカル表示経路。配備済み Rust・OIDC・live Drive のブラウザー成功とは区別する。

確認画面: [ローカル実データ MOC](http://127.0.0.1:5185/app/job-copy?data=actual)。この preview は事前取得・検証済み原本を配信する。`tests/e2e/job_copy_drive_images.spec.ts` の専用 UI E2E **2 件成功**。live Drive の可用性や本番認可を検証した結果ではない。

残作業は本番保存先・認証権限・採用 ADR、単一 writer を保証する scheduler／lease、日次の媒体観測と原本確保、Retry/Pending 運用、実アプリ配備とログイン付き画面検証。既存 HubSpot 同期へ接続する際は、媒体のタイトル／状態変更だけを画像取得の起動条件にしない。本文・画像だけの変更を独立して観測する。

現在の画像取得は初回標本で約 3–5 秒を要している。参照解決の繰返しや取得の待ち時間を測定し、認可・hash 確認を維持した改善を統合窓口で検討する。現段階で性能改善・本番配備を完了扱いにしない。

根拠は非公開 `data/job-copy-local/drive-experiment/` の upload／readback／manifest／recovery／integration 結果。Drive 関連 Python offline テスト 37 件が成功。限定結果・API・本番運用の境界は [検証計画](job-copy-drive-pilot-plan.md) と併せて確認する。

## 2026-10-05 継続構築・移行準備の追加

上記の 2 求人・3 原本は初回の実連携結果として保持する。ユーザー方針により現在の実験フォルダを継続利用し、将来の移行に備えて保存先設定を分離した。[保存先・移行契約](job-copy-drive-storage-contract.md) と [継続構築計画](job-copy-drive-continuation-plan.md) を今回の追加契約として参照する。

`identity_folder_id` は初回の操作を識別する固定値、`folder_id` は現在の物理保存先。保存・親フォルダ検査・候補列挙は物理 ID を使い、操作 hash と既存 receipt の `folder_id` は固定 identity で照合する。3 つの Python CLI に `--identity-folder-id` を追加した。省略時は従来どおり物理 ID を使うため、既存原本 ID・manifest ID・schemaVersion 1・原 bytes・hash は維持される。

同じ ID のまま原本と manifest を別フォルダへ移した模擬環境では、固定 identity と既存 receipt により CREATE 0・同じ ID・原 bytes 一致・復旧を確認した。identity の誤設定や部分移動は停止する。フォルダ自身を同じ ID のまま移す場合は設定 ID を維持する。Drive Python テスト **44 件成功**はこの模擬移行を含む結果であり、実共有ドライブへの移動・権限移行を実施した証拠ではない。

`scripts/job_copy_archive_batch.py` を追加し、原本検証→内容 hash による重複排除→予約 ID を使った保存→原 bytes 読取→manifest 保存をまとめた。画像・manifest の checkpoint に加え、`summary.json` の `runId` と `state=running/succeeded/failed` で各実行を識別する。以前の成功結果を新しい失敗の証拠に使わない。この状態管理を含むバッチ用テスト **6 件成功**。バッチは HubSpot・共有権限・ファイル移動を書き換えない。

追加レビュー版は **36 求人・応募レコード 317 件**。その画像がある対象 **28 求人・45 参照・内容の異なる原本 34 件／7,130,985 bytes**について、現在フォルダへの保存と読み戻しを完了し、28 manifest を検証した。既存原本 3 件・manifest 2 件を再利用し、新規 CREATE は原本 31 件・manifest 26 件。この段階では **全 28 求人の live HubSpot 参照登録完了を認定していない**。再実行と Rust publish／参照取得の結果を次の検証として追記する。初回バッチは状態表示修正前のコードで完了したため、state のない旧 summary だけを新形式の実行完了印として扱わない。

アプリ設定の引継ぎ artifact は非公開 `data/job-copy-local/applicant-review/expansion/app-image-settings.env`。`JOB_COPY_MOC_PATH` は原本レビュー用 `data/job-copy-local/applicant-review/review-moc.json`、`JOB_COPY_DRIVE_LISTINGS` は対象 28 求人の allowlist。元 JSON は保持し、実行時に対応する hash・slot の画像 URL を解決する。artifact の存在だけでは本番環境への設定反映・配備を意味しない。

operator のアップローダーは **gws のユーザー OAuth**、Rust と原 bytes の読み戻しは **サービスアカウントの Drive readonly**を使う。両者の認証・対象フォルダ権限は別であり、ユーザーの CLI 成功をサーバー側権限の成功と同一視しない。復旧も manifest 候補の再構成であり、完了判定は HubSpot current/history と全原本の照合で行う。

定期 scheduler、複数 writer の lease／排他、本番運用の Pending 回復監視は引き続き未実装。既存同期は別セッションの所有を維持する。今回の追加でも commit・deploy・実ファイル移動は行っていない。

## 28求人拡張の完了結果

上記の拡張処理について、全28求人の実HubSpot参照登録・Rust経由の画像45件取得が完了した（新規pointer更新26件、既存一致2件）。Drive保存の再実行は原本・manifestともCREATE0、Driveからの復旧候補は28件・45画像参照で保存済みID/hashと一致した。

HubSpot連携も28求人を再実行し、全件 `already_linked`、求人pointer追加更新0件を確認。再取得画像45件の原bytesもhash一致した。

確認画面は[拡張レビュー版](http://127.0.0.1:5187/app/job-copy?data=actual)。36求人・317応募を保持し、28求人の45画像参照を保存済みDrive原本に接続した。ローカルプレビューのE2E2件とPC/スマートフォン幅の目視確認を完了。これはRustによる実取得済み原本を配信する画面であり、配備済みアプリのOIDC検証とは区別する。

統合担当は[拡張結果と残作業](job-copy-drive-expansion-results.md)を参照すること。画像・応募データはGit管理外のため、コードの取り込みだけでは確認画面のデータ配置や環境変数の反映は完了しない。

## 既存アプリへ取り込むための追加

レビュー用JSONも現在の非公開Driveへ配置し、RustからID/hash指定で取得する経路を追加した。オンライン環境は旧 `app-image-settings.env` のローカルパス指定に依存せず、新しい `app-integration/server-settings.env` のサーバー設定を使う。画像参照の解決は最大4求人並行で行う。

実配置・読み戻し・既存認証付きAPI Routerからの36求人/317応募/45画像参照の取得を検証した。設定と検証範囲の最新情報は[アプリ組み込み引継ぎ](job-copy-app-integration-handoff.md)が正本。配備済み本番のGoogleログイン確認とは区別する。
