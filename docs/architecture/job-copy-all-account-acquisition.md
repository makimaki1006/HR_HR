# 顧客全アカウントの収集への修正（2026-10-07）

前回は単口座CSVとHubSpotに存在する求人URLのサンプル観測で止めていた。全顧客アカウントの取得完了ではない。既存アカウント情報シートを母集団にして全対象を巡回する。

## AirWorkの対象

- 企業B系ID/PWが揃う稼働対象561行、同一ログインIDを除いて519アカウント。
- 重複42行、同一IDの資格情報競合0。顧客数とログインアカウント数を同一視しない。
- クローズ扱いで資格情報がある402行は今回の稼働対象巡回から除外。資格情報がない顧客も取得成功に含めない。
- HubSpot検索に現れた404口座とは別の母集団。完全一致330、稼働対象シートのみ189、HubSpotのみ74。未対応は削除と断定しない。
- 応募発生を起動条件にせず、全519アカウントの台帳にpending/取得中/成功/求人なし/失敗を保持する。

## 実行

既存AirWork fetch_aw_xlsxを直接呼び、同時4口座で全対象のXLSXを取得中。既存の取得成功1口座は再利用。各口座の原本、本文全列、原本生成日、実取得日、件数を残す。取得成功した口座から、59求人ずつのバッチで画像を2口座並列に収集する。CSV4口座と画像2口座が同時に進行する。

応募同期orchestratorは実行しない。共有の本番cursor/queue/result、通知、HubSpotレコード、Drive、本番スケジュールは変更しない。PWは保存台帳・ログへ出力しない。認証session/cacheと顧客原本は既存Git除外ディレクトリのみ。

作業場所は job-copy-load-recovery worktree の data/job-copy-local/recovery/airwork/。
- acquire_all_accounts.py：再開可能な取得専用runner。
- all-accounts-process.json：実行PID。実行PIDは非公開のall-accounts-process.jsonを参照。
- all-accounts-parallel-v2-run.log：秘密値を含めない件数経過。以前のログも保持。
- live-2026-10-07/all-accounts/summary.json：実際の進捗。
- live-2026-10-07/all-accounts/accounts.json：非公開口座別台帳。
- live-2026-10-07/all-accounts/account-population-reconciliation.json：母集団の差異。
- 各口座hashディレクトリ：原本・本文・session/cache、画像観測。

同一出力先はOS lockで排他する。実行中に重複起動しない。停止後は同じrunnerを再開し、成功CSVは再取得せず未着手から処理する。失敗口座を件数から消さず、認証失敗の無制限再試行はしない。pass_completeは巡回終了であり、全口座成功を意味しない。

本書は実行開始時の方針・対象を記録している。件数の最新値はsummary.jsonを読む。全媒体の全顧客収集完了やアプリ反映完了とは報告しない。

## 台帳確認の補足

全台帳で企業資格情報があるログインIDは900種類、今回の非クローズ巡回対象は519種類。全台帳の900種類と稼働対象の519種類を混同しない。対象561行のうち560行はクローズFALSE、1行は状態空欄で既存loaderが対象に含めている。

非クローズの資格情報欠損702行にはチェックボックスだけの520行が含まれる。実値がある資格情報欠損は182行で、AirWorkを使う顧客か、HRのみか、資格情報整備が必要かを確認する。これを取得成功・取得対象なしと決めつけない。

HRHackerはこのAirWork口座リストとは別の構造で、既存管理認証1組から店舗群CSVを取得する。HR掲載タブの1,593求人行を1,593口座と数えない。

private証跡は data/job-copy-local/recovery/airwork/account-inventory-readonly.json。現行の全口座取得はAirWorkの519対象を進行中で、HR全件再取得や全媒体の完了とは報告しない。
## 並列実行と時間計測（追記）

CSV4 workers＋画像2 workersを同時実行。台帳はイベントループ内の単一writer、画像原本は口座/実行/batch別ディレクトリ。再開時は原本ZIPのSHAだけでなく本文全行・全列との一致を確認する。画像枚数の空欄は不明として扱い、0を補完しない。2GiBの空き容量を残すため取得開始・画像GET時に確認する。

全体開始は2026-10-07T10:31:40.264189Z（日本時間19:31:40）。summary.jsonのtimingにstartedAt、CSV巡回終了csvPassCompletedAt、全巡回終了completedAt、wallElapsedSecondsを保存する。全体時間は再起動の停止時間を含む。各口座のdownloadAttempts/imageRunsにも開始・終了・実測durationSecondsを保持する。再利用済みの原本を新規ダウンロードの実測値として扱わない。

4＋2の合成並列検証では最大download4/image2を実測し、成功CSVの再利用・本文改変の拒否・時間計測を確認した。これは媒体実接続の全件成功の証拠ではない。実接続でもdownloading4/image_capturing2の同時進行を確認済み。画像枚数空欄で失敗した口座は本文原本の照合後に画像収集へ再投入した。

全519口座の一巡は2026-10-07 20:56:31（日本時間）に終了。実測84分51.255秒、CSV取得成功302口座・5,795求人、求人なし8口座、取得失敗209口座。画像観測5,795求人・保存画像1,760参照。pass_completeは一巡の終了であり、全成功ではない。詳細と次の取得契約はjob-copy-media-acquisition-handoff.mdを参照。

[両媒体の取得契約](job-copy-media-acquisition-handoff.md)に、正本の取得関数、最新の全口座巡回結果、画像保存、失敗・再開・計測の手順を集約した。
