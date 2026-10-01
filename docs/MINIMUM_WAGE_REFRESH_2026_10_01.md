# 最低賃金の2026年度改定

## 反映範囲

公式の県別改定額と発効日を `data/minimum_wage_rates.csv` に保存する。2025・2026年度各47県を収録し、Rustの共通処理 `src/minimum_wage.rs` が日本時間の当日までに発効した額を選ぶ。競合調査、Indeed職種、地図、求人生成、公的統計、給与妥当性で共通処理を利用する。DockerビルドにもCSVを同梱する。

2026年10月1日時点では15都道府県が新額、32県が旧額。大阪府は1,231円、東京都は1,280円。京都府は11月15日まで1,122円、11月16日から1,180円。沖縄県は12月1日まで1,023円、12月2日から1,086円。全県を10月1日に一律更新してはいけない。

出典:

- [厚生労働省・全国最低賃金一覧](https://saiteichingin.mhlw.go.jp/table/page_list_nationallist.php)
- [2026年度の県別改定一覧](https://www.mhlw.go.jp/content/11200000/001753406.pdf)
- [2025年度の県別一覧・過去の全国平均](https://www.mhlw.go.jp/content/10200000/001646434.pdf)

全国平均1,177円は2026年度の全改定を反映した加重平均であり、10月1日の新旧混在した全国平均ではない。2025年度の全国平均は1,121円。年度履歴と現行額を分けて扱う。

## DB更新はユーザーが実行する

`CLAUDE.md` のTurso書き込み規則に従い、エージェントはDBに書き込まない。生成スクリプトもSQLの保存と読み取り専用のスキーマ確認だけを行う。

まずバックアップを取得し、設定済みの外部統計DBを読み取り専用で確認してSQLを生成する。接続設定は `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN`。別用途のCockpitやRecLogのDBに接続しない。

```powershell
python scripts/update_minimum_wages.py --as-of 2026-10-01 --turso-inspect --output target/competitor-preview/minimum-wage-update.sql
```

接続設定がない場合も、次のコマンドでレビュー用SQLは生成できる。ただしスキーマは未確認になる。

```powershell
python scripts/update_minimum_wages.py --as-of 2026-10-01 --output target/competitor-preview/minimum-wage-update.sql
```

ユーザーがSQLを確認して対象DBのクライアントで実行する。失敗時に停止・ロールバックする設定を使う。現行テーブルの県キーと履歴テーブルの年度・県の一意制約が必要。SQLは既存行をUPSERTし、テーブルや過去年の行を削除しない。履歴に発効日列がある場合は、読み取り専用のスキーマ確認を付けて生成することで公式日付も保存する。

現行表は指定基準日の有効額のみを保存する。後日発効する県のDB更新には、その発効日以降の `--as-of` で再生成が必要。アプリの公式CSV参照は日付に合わせて自動で切り替わる。

履歴だけを生成する旧スクリプトは、現行表の更新には使わない。現在は履歴を壊さないSQL生成専用に変更している。

## 派生データと確認

現行・履歴更新後は、最低賃金を複製している給与遵守率、地域ベンチマーク、月次比較、生活コストproxyも再生成する。月次比較は月末基準、生活コストproxyは物価統計年と最低賃金の基準日・年度を区別する。全パイプラインの無条件実行や古い10月一律切替処理の利用は避ける。

派生テーブルの限定更新SQLも、次のコマンドでレビュー用に生成できる。DB書き込み機能はない。対象DBのテーブル・列を確認し、ユーザーが各DBクライアントで実行する。既存SQLiteのコピーがある場合は `--schema-db <コピーのパス>` を付けると読み取り専用で列を確認する。

```powershell
python scripts/revalue_minimum_wage_dependencies.py --as-of 2026-10-01 --tables compliance benchmark --output target/competitor-preview/wage-derived.sql
python scripts/revalue_minimum_wage_dependencies.py --as-of 2026-10-01 --tables cross --output target/competitor-preview/wage-cross.sql
```

`wage-derived.sql` は `postings`、`v2_wage_compliance`、`v2_region_benchmark` が同じDBにあることが前提。給与遵守率は元の時給求人・雇用区分で再集計し、同じ入力から下回る件数・比率・平均・中央値を更新する。保存済み求人件数と再集計件数が変わった行は更新せず、末尾のSELECTで報告する。こうした行は新しいスナップショットの全面再集計が必要であり、賃金額だけの更新に混ぜない。地域ベンチマークも更新を検証できた対応行だけ変更する。求人件数・給与分布が変わった場合はSQL末尾の検証を省略しない。

`wage-cross.sql` は外部統計DBの既存 `cross_wage_public` の最低賃金列だけを更新し、給与統計列は保持する。月末までに発効した県別額を用い、日数按分ではない。公式CSVで発効日が確認できない古い期間は既存額を保持する。全国の既存年度参考額も保持するが、混在月の現行法定額として表示しない。新規のクロスCSV生成では、全国と発効日根拠のない期間の最低賃金を欠損にする。年度参考額から月次法定額を推測しない。

生活コストproxyのSQLは、必要な地域マスタと物価統計を持つ既存SQLiteを読み取り専用で開いて生成する。次の例では `data/hellowork.db` が必要。この作業環境には同DBがないため、このコマンドは実行していない。

```powershell
python scripts/build_municipality_living_cost_proxy.py --db data/hellowork.db --dry-run --as-of 2026-10-01 --output target/competitor-preview/wage-cost.sql
```

proxyの `source_year` は計算スナップショット年。`source_name` に物価統計年（現行入力は2024年）、県別の最低賃金年度・発効日・計算基準日を別々に保存する。既存の過年度スナップショットは削除しない。物価統計自体を改定した場合は、その統計年も確認して変更する。

生成SQL末尾のSELECTで47県の現行額・年度・発効日と年度別履歴件数を照合する。大阪・東京の新額に加え、京都・沖縄の発効日前の旧額も確認する。画面では最低賃金の年度・発効日・基準日・出典を確認する。未取得の人口・労働統計は最低賃金から推測しない。

差し戻す場合はバックアップと更新前の県別値を用いる。将来額の年度行を削除して旧額へ戻す方法は採用しない。

初回のローカル実装時は外部統計DBの接続設定がなく、本番スキーマは未確認だった。その後のユーザーの「Tursoも更新してほしい」という明示依頼により、この最低賃金更新について本番書き込みが許可された。RenderのHR_HWサービス（hr-hw.onrender.com）に登録された設定からcountry-statisticsへの接続を確認した。トークンはログやファイルへ保存していない。Gitへの公開・Renderへのコードデプロイは未実施。

## 本番Tursoの更新結果（2026年10月1日）

更新前の対象テーブルを `target/competitor-preview/wage-live-backup.json` に保存し、その実データ・スキーマのコピーで同じSQLを検証した。2026年10月1日を基準に、条件付きCOMMIT／失敗時ROLLBACKの1トランザクションで更新した。再送はしていない。

- 現行表: 47県の額・改定年度・実際の発効日を更新。15都道府県が2026年度額、32県は発効前の2025年度額。
- 履歴表: 151行から199行。2026年度48行（47県＋全国）を追加し、2025年度全国平均を1,113円から1,121円へ訂正。2024年度以前の全行を保持。
- `v2_external_prefecture_stats`: 47県の最低賃金列のみ更新。他の指標は保持。
- 生活コストproxy: 2026年基準の1,917地域を追加。2025年の1,917行は変更せず保持（合計3,834行）。物価指数を変えずに最低賃金の実質換算を再計算し、年度・発効日・基準日を出典欄に記録。
- `cross_wage_public`: 2025年1〜12月の576行だけが収録されているため、今回の2026年度改定では全行を保持。
- 給与遵守率・地域ベンチマーク・元求人テーブルは同Tursoに存在しないため書き込み対象に含めていない。

COMMIT後に実DBをSELECTし、47県全件の額・年度・発効日、履歴、関連指標、過去行の保持を更新前バックアップと照合した。東京1,280円・大阪1,231円、発効前の京都1,122円・沖縄1,023円を確認済み。

証跡: `target/competitor-preview/minimum-wage-live-update.sql`（実行SQL）、`minimum-wage-write-receipt.json`（コミット・更新件数）、`minimum-wage-verified.json`（照合結果）、`wage-live-after.json`（更新後SELECT結果）。これらは作業用の非追跡ファイル。

生活コストの取得SQLは、自治体・basisごとに最大source_yearだけを選ぶようローカルで修正した。保存した実DBの更新前・更新後データで実際のRust SQLを検証し、旧年度のみの場合と新旧年度混在の場合の双方で1地域1行、更新後の大阪1,231円を確認した。Renderのコードはまだデプロイしていないため、既存の生活コスト画面が旧年度も取得する点はこのSQL修正のデプロイで解消する。最低賃金の現行テーブル自体の更新はコミット済みである。

## ローカル検証結果

2026年10月1日に公式サイトの47県の改定額・発効日と前年額を独立照合した。Rust全libテストは3,634件成功・失敗0・45件ignored、Pythonの最低賃金・SQL更新テストは11件成功。ブラウザでは幅320〜1,600pxの6種類で4タブの切替、キーボード操作、印刷、横方向のはみ出しなし、大阪府1,231円と発効日・出典表示を確認した。これらは本番DB更新や外部API接続の検証を意味しない。

結果プレビュー: `C:\Users\fuji1\orca\workspaces\HR_HR\gurnard\target\competitor-preview\report.html`
