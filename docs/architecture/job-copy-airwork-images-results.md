# AirWork画像取得：実装・利用・引継ぎ

HRハッカーを含む全件取得・両媒体の受渡し契約・2026-10-07の519口座巡回結果は [両媒体の取得ロジック](job-copy-media-acquisition-handoff.md) に集約した。本書の少数求人検証は実装初期の結果で、現在の全件巡回結果とは区別する。

2026-10-07。既存求人取得の `aw_recruit_urls.json` と `build_aw_job_url` の構成を再利用し、公開求人の画像取得を追加した。実装はHR_HR `scripts/job_copy_airwork_images.py`。既存HubSpotプロジェクトのfetcher/importer、Rust認証、React画面は変更していない。

## 取得経路

既存口座別URLキャッシュ＋XLSXの `求人番号(job_offer_id)` → `https://arwrk.net/recruit/<slug>/<求人ID>/` → canonical一致と `.job-img .slider` ギャラリーを確認 → `cdn.arwrk.net/images/rct/` の画像を順にGET → 原本SHA-256別に非公開保存 → 求人/slot/参照hash/実取得日時を記録 → 完全取得かつCRM対応入力がある求人だけ既存Driveバッチ用samplesを生成。

一覧・会社ロゴ・thumbnail・slide cloneは画像に含めない。本文変更を起動条件にせず、呼び出しごとに現在bytesを読み取る。同じURLの差替えでもcontent hashが変わる。媒体観測日と画像取得時点は別で、過去画像は復元しない。

画像CDNは実測で `binary/octet-stream` を返した。固定許可CDNからのoctet-streamだけを許容し、Pillowの実byte検証でJPEG/PNG/WEBPを確定する。明示された画像MIMEが実形式と異なる場合は拒否する。page2MiB、画像5MiB、3000万画素、100画像、1バッチ59求人の上限がある。redirect、任意host、userinfo/port、制御文字URL、別求人canonicalを拒否する。HTTP404/410は `public_page_unavailable`、HTML構造変更は `gallery_layout_unrecognized` として区別。

## 入力と実行

以下は合成の契約例。`accountKey` は既存URLキャッシュのキーと厳密一致させる。IDやキャッシュは非公開ファイルで扱う。Company/Listing対応は既存のHubSpot関連読取で確認した値を渡す。このcollector自身はHubSpotを照会して対応を証明するものではない。

```json
{
  "schemaVersion": 1,
  "jobs": [
    {
      "accountKey": "<既存口座キー>",
      "mediaJobId": "12345678",
      "hubspotListingId": "123",
      "companyIds": ["456"],
      "expectedImages": 4
    }
  ]
}
```

`expectedImages` はXLSXの `募集イメージ数(photo)` を整数化した申告数。未取得なら省略/nullで、0を作らない。古いXLSXの枚数と現在ページが違う場合は `declared_image_count_mismatch` のpartialにし、掲載画像を勝手に増減確定しない。

```powershell
python scripts/job_copy_airwork_images.py --input <非公開求人対応.json> --recruit-cache <既存aw_recruit_urls.json> --output <非公開観測ディレクトリ>
```

キャッシュを直接使わないadapterは `recruitSiteUrl` を各入力求人へ付けて `--recruit-cache` を省略できる。口座とサイトの曖昧対応を拒否し、採用サイトを別口座から推測補完しない。HubSpotからの `id_airwork` / `airwork_account_login_id` とXLSX口座/求人の対応は呼出し側で照合する。

実装を既存Python取得工程から呼ぶ場合：

```python
from job_copy_airwork_images import from_recruit_cache, capture_jobs
payload = from_recruit_cache(account_scoped_jobs, existing_recruit_url_cache)
result = capture_jobs(payload, private_observation_output)
```

ネットワーク操作は公開ページ/画像のGETだけ。保存済み媒体sessionを読み込まず、媒体へのログイン・求人編集・CSV再生成・HubSpot更新・Driveアップロードを起動しない。

## 出力・失敗・保存への接続

- `originals/<SHA>.jpg|png|webp`：取得原bytes。縮小・再エンコードしない。同bytesはファイルを再利用。
- `pages/<SHA>.html`：今回取得した非公開のページ証跡。顧客本文を含むためGit/公開artifactへ置かない。
- `capture-results.json`：running/complete/partialと求人別結果。結果のimagesにslot、URL参照SHA、原本SHA、形式、bytes、保存path、取得日時を記録。口座/求人IDを含むため非公開。
- `archive-samples.json`：既存 `job_copy_archive_batch.py` のsamples契約。取得完全かつListing/Company入力がある求人の画像だけを含む。未対応/部分失敗の求人は原本を保持しても登録用samplesに入れない。

CLI終了コード0は全入力の公開ギャラリー取得成功、2は一部失敗/取得不能、1は入力・実行失敗。申告枚数不明の場合のcompleteは「確認した公開ギャラリー全画像の取得」で、媒体の全画像・非公開求人・全口座網羅の意味ではない。samplesのある求人だけ登録した場合、元入力全件の連携成功とは報告しない。

開始時に古いsamplesを空にし、原本/結果はatomic保存。同じ出力先は既存operator用OSロックで排他し、求人ごとにcheckpointする。途中で停止したrunning結果を成功と扱わない。原本はURL失効後も保持される。これはローカルoperator取得証拠であり、本番の日次scheduler/耐久Pendingの完成ではない。

全件結果・対応を確認した後、既存保存処理に渡せる：

```powershell
python scripts/job_copy_archive_batch.py --samples <archive-samples.json> --storage-config <既存非公開設定.json> --service-key <SA鍵> --output <非公開保存結果> --upload
```

空samplesは既存バッチが拒否する。Drive保存後にRust `job_copy_drive_link` で関連・原本を確認して専用pointerを登録する。保存成功だけでHubSpot登録・画面表示まで完了と扱わない。今回はDrive/HubSpot書込みは実行していない。

## 実検証と範囲

既存AirWork取得の1口座・取得済みXLSXを根拠に、既存口座別サイトキャッシュから3求人の公開URLを確認。1求人HTTP200、画像JPEG **1件・31,278bytes**を保存し、原本hash一致。ほか2求人は404でunavailable、画像0の成功求人へ変換しなかった。媒体の全求人や非公開求人の取得成功ではない。保存データは `data/job-copy-local/recovery/airwork/` のみ、公開ソースに原URL/顧客名/口座/画像を含めない。

offlineテストで、ギャラリー順序、clone/thumbnail除外、4画像全slot保持、誤canonical、口座不一致、URL制御文字、全行preflight(GET0)、redirect、404、サイズ/画素/MIME不正、同URL別bytes、部分失敗、途中停止、既存Drive manifestとの契約一致を確認。確定テスト件数は実行ログと統合依頼で報告する。

## 全求人連携担当へ残す作業

1. 既存AW取得完了時に、口座キー・XLSX求人ID・画像申告数・CRM関連をこのcollectorへ渡す。独立した全口座ダウンロードを増やさない。
2. 非公開/終了求人で公開ページを読めない場合、既存認証済み管理画面から画像を読むadapterを別途調査する。404を求人削除とみなさない。
3. 取得原本を既存Drive不変保存へ接続し、HubSpot pointer更新・読み戻しまで実施する。定期実行と永続Pendingは既存同期担当と設計する。
4. AirWork本文と応募を表示データへ戻す。画像未取得でも本文を除外しない。
5. 現行Rust/Reactのcapture契約は画像3点まで。本collector/Drive manifestは4点以上を保存できるが、画面全画像表示の対応は別変更。省略して成功扱いにせず、上限拡張・ページ表示の契約を揃える。

全件化の方針は `job-copy-all-listings-handoff.md`、保存・将来移行は [Drive保存契約](job-copy-drive-storage-contract.md) を参照する。
