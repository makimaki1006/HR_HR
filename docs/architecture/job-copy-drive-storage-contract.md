# 求人画像の保存先と移行契約

2026-10-05。ユーザーの方針により、現在の個人所有の実験フォルダで構築を継続し、将来の保存先移行に備える。現時点で本番フォルダを確定したものではない。

## 保存先の設定

バッチの非公開設定ファイルは次の形。鍵やトークンは含めない。

```json
{
  "schemaVersion": 1,
  "folderId": "<現在の保存先フォルダID>",
  "identityFolderId": "<初回フォルダIDを使った固定の識別値>",
  "imageReceiptDir": "<既存画像receiptディレクトリ>",
  "manifestReceiptDir": "<既存manifest receiptディレクトリ>"
}
```

`folderId` は実際の保存・親フォルダ検査・復旧候補の列挙に使う。`identityFolderId` は過去の操作IDと予約IDを継続利用するための固定値。現在は両者が同じだが、異なる保存先へ中身を移す際には `folderId` だけ変更する。固定値は移行後も変更しない。

既存receiptの `folder_id` は互換上の名前を保持し、固定の識別値を表す。現在の物理保存先は設定から取得する。manifest schemaVersion 1、原bytes、原bytesのSHA-256、既存HubSpot参照には変更を加えない。

## バッチ処理

```powershell
python scripts/job_copy_archive_batch.py --samples <原本と求人の対応.json> --storage-config <保存先設定.json> --service-key <既存SA鍵.json> --output <非公開実行結果ディレクトリ> --upload
```

1. 原本・hash・サイズ・画像形式・求人ごとのslot重複などをローカルで検証する。
2. 原本を内容hashで重複排除し、既存receiptにあるDrive IDを再利用する。新規分はIDを予約・保存してからCREATEする。
3. アプリのサービスアカウントで原bytesを読み戻し、全画像の保存完了後に求人別manifestを検証・保存する。
4. 画像／manifestのcheckpointを逐次保存する。`summary.json` の `runId` と `state` で開始・成功・失敗を区別する。古い成功結果を新しい失敗の成功証拠にしない。
5. HubSpot登録は既存のRust CLIから実行する。PythonバッチはHubSpot・共有権限・ファイル移動を変更しない。

同じreceiptディレクトリを使う。別のディレクトリへ切り替えたり、予約記録を消して再実行した場合の全体重複排除は保証しない。ローカル予約記録はこの実験の再実行用であり、本番の冗長ストレージや複数writer排他の実装とは区別する。

Driveに保存済みのmanifestは、ローカルreceiptがなくても次で復旧候補を再構成できる。

```powershell
python scripts/job_copy_drive_recover.py --folder-id <現在の保存先ID> --identity-folder-id <固定の識別値> --readback-service-key <既存SA鍵.json> --output <非公開復旧候補.json>
```

復旧候補は未反映リストとは限らない。RustがHubSpotの現在参照と過去の参照経路を照合して、未反映・既反映を判定する。候補の列挙成功だけで全画像の移動完了を判断しない。manifestの全画像を検証する保存処理と、Rustの原本取得まで通す。

## 将来の移行手順

1. バッチの書込みを止め、現在設定・receipt・manifest・原本の一覧を保全する。
2. 移動先の権限と移動方法を確認する。コピーや再アップロードでIDを変える方法は、この手順の対象外。
3. 同じファイルIDを保持して原本とmanifestを移す。フォルダごとの移動が可能で同じフォルダIDを保持する場合は、設定IDを変更しない。
4. 中身を別フォルダへ移した場合は `folderId` を変更し、`identityFolderId` とreceiptの場所は保持する。
5. サービスアカウントの取得権限、全ファイルの親・原bytes、CREATE0での再実行、復旧、HubSpot参照経由の表示を確認する。
6. 不完全な移動・identity誤設定・原本不一致では止める。全確認後に定期処理を再開する。

移行後の挙動は模擬テストで確認した段階。実共有ドライブへの移動・権限移行・本番ジョブ再開を実施済みとは扱わない。
