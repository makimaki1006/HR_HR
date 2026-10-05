# 求人文面React画面の合流候補と検証結果

2026-10-05。基準mainは `da81c557`、候補ブランチは `makimaki1006/job-copy-react-join`。求人文面のReact実装を、最新のRust/Axumアプリの共通AppShellとナビへ追加した。入口は「求人文面（MOC）」→ `/app/job-copy`。

共有API client、認証、CRM router、既存画面、Dockerfileは維持した。日次の求人・応募同期を別セッションから引き取っていない。顧客データ・画像・資格情報をGitやDockerへ収載していない。Driveの保存先も変更していない。

## 検証

| 検査 | 結果と範囲 |
| --- | --- |
| Rust全体 `cargo test --lib` | 再実行で3,978成功・失敗0・50ignored。初回のナビ末尾期待値1件を、新導線と既存相対順の検査へ更新した |
| ルート競合・起動契約 | 32成功。求人画面の未認証303、求人APIのJSON401/no-storeは全体unitにも含む |
| 静的検査 | Rust fmt、Clippy all-targets、および追加operator exampleのClippyが成功。既存警告は残る。CSS契約7成功、統計表現lintとCSS監査selftestも成功 |
| React | 型チェック・全Lint・build成功。求人文面134単体、共通API/Shell・CRM等の関連テスト、追加の既存26ファイル328回帰テストが成功 |
| ブラウザ | fixture E2E 11成功。本文完全一致、取引先絞込、比較タブ、375px、画像bytes/hash、印刷、7種類の失敗案内を確認 |
| 実クラウド読み取り | Driveの不変JSONとHubSpot参照から36求人・317応募・45画像参照を取得。テスト内で作成したOIDCセッションによるAPI router検証1成功。取得は21,251msの1回測定 |
| 保存operatorのoffline検証 | 59成功・1skip。skipは候補へコピーしていない実private原本の検査 |

ブラウザ検証は候補のビルド済みReactと共通AppShellを使用する。ナビはfixtureであり、画像は合流元で取得・照合した原本を読み取り配信した。実クラウド検証でもGoogleへのログイン操作は行っていない。本番OIDCログイン、配備後の画面、A4 PDF改ページ、永続的なDrive可用性の証明とは区別する。

フロントエンド全体suiteは未完了。既存Sales KPIの `parity.card.test.tsx` と `parity.nt.test.tsx` が長時間終了せず、一括実行と時間制限付き追試を終了した。該当コード・fixture・test設定はmainから変更していないが、原因は未確定。全体suite成功とは報告しない。

## 逆証明と修正

- 不正な画像allowlistを黙って除外すると、クラウド設定があるのにinline画像へ戻り得た。現在は不正値・空要素を503とし、現在画像のある全求人がallowlistで網羅されないクラウドsnapshotも503にする。未認証者には先に401を返す。
- 求人画面に旧共通API clientをコピーするとPOST等の最新契約を巻き戻すため、共通clientは変更せず、安全なエラーコード解釈を求人画面側へ限定した。
- 印刷で全体317応募と選択求人の応募数が混ざっていたため、共通ナビ・全体取得集計・live読み取りパネルを除外した。選択求人の応募数、版対応の未確定注記、全文は保持する。
- 現在の45画像参照は34原本。過去画像原本は0件で、未観測画像は生成しない。応募314件は版対応不明、残り3件も日付観測に基づく暫定対応。変更による応募構成の因果効果は断定しない。

## 統合窓口への次の作業

既存のReact移行・統合担当を唯一のmerge窓口として候補を渡す。本候補側ではmergeや本番deployを行っていない。取り込み後、既存のサーバー資格情報と求人管理用環境変数を登録して再起動し、実際のGoogleログインから本文・画像・認可を確認する。

設定の元は合流元gurnardの非公開 `data/job-copy-local/applicant-review/expansion/app-integration/server-settings.env`。JSONのDrive IDとSHA256を必ず対で更新する。詳細は[組み込み引継ぎ](job-copy-app-integration-handoff.md)と[保存先・移行契約](job-copy-drive-storage-contract.md)を参照する。日次観測の自動接続、永続Pending、複数writer制御、レビュー側の過去manifest解決は別途必要。

今回の証拠は候補の非公開 `data/job-copy-local/integration-validation/` と `data/job-copy-local/candidate-browser/` に保存した。顧客スクリーンショットや実データJSONは公開PRへ添付しない。
