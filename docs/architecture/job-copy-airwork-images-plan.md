# AirWork画像取得の追加計画

2026-10-07。既存HubSpotフォルダの `aw_csv_fetcher.py::extract_aw_recruit_site_url` / `build_aw_job_url` / `aw_recruit_urls.json` を参照した。XLSXは画像枚数だけなので、既存の口座別採用サイトURL＋求人IDから公開詳細HTMLをGETし、求人ギャラリーの画像を取得する。

実読取で1ページHTTP200、canonicalが要求求人URLと一致、`.job-img .slider` に画像1件、配信host `cdn.arwrk.net` を確認。同じ口座の別2ページは404。XLSXの掲載状態や取得成功だけで現在の公開ページの存在を保証しない。

追加はHR_HRのoperator用Python collector。既存HubSpot同期・媒体編集・Rust認証・React構造は変更しない。入力は既存取得側から渡す口座キー、採用サイトURL、求人ID、任意のHubSpot Listing/Company対応、任意の申告画像枚数。口座/求人キーとURLを検証し、全行preflight後に取得する。生のHTML・URL・画像・口座IDは非公開出力に限定する。

HTTPSの正規 `arwrk.net/recruit/<slug>/<求人ID>/` のみ読み、canonicalを厳密照合。求人ギャラリーだけを抽出し、thumbnail・ロゴ・slide cloneを数えない。画像は実測した `cdn.arwrk.net/images/rct/` のみ。redirectは追わず、page2MiB/画像5MiB、タイムアウト、JPEG/PNG/WEBP、画素上限を設ける。HTML構造変更や404、取得失敗、申告枚数差を状態として返す。取得失敗を画像0件・削除・変更なしにしない。

現在画像は本文差分を待たず毎回bytesを取得する。原本を内容SHAで保存し、観測日時・JST日付・URL参照SHA・slotを保持。本文と画像の時点を混同せず、過去原本を作らない。Listing/Company対応が検証済みの完全取得求人のみ、既存 `job_copy_archive_batch.py` に渡せるsamplesを生成。部分取得の求人を完全manifestとして登録しない。Drive upload・HubSpot pointer更新は別の明示工程とする。

検証：HTMLのギャラリー/サムネイル/clone、canonical別求人、口座とURL不一致、SSRF/redirect、同URLバイト更新、slot順変更、404/巨大応答/画像不正/部分失敗、既存Drive manifest契約への接続をofflineテスト。確認済み公開求人を少数GETして原本hash/形式/件数を実検証する。非公開求人の管理画面画像取得は、この公開経路で成功したと扱わない。
