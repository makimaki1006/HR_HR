# 求人文面React画面の最新アプリへの合流

2026-10-05。ユーザーの指示によりサブエージェントでReact・認証依存・データ配置を並列確認する。統合窓口は既存の担当セッションへ一本化する。

## 基準と差分

基準mainは `da81c557f01810338379eaa45436c1f1204a7ec5`。元のgurnard作業には旧シェル・API client・CRMメタデータの実装と競合調査などの差分が混在している。mainの共通App Shell、ナビ、CRM router、AppState.hubspot、401JSON、CSRF、既存画面を保持して必要差分だけ追加する。

## 所有と順序

1. root: job-copyのRustモジュールと依存、登録、共通ナビの追加、設定、全体検証と統合成果。
2. React担当: frontendの求人文面コンポーネント、専用エントリ、最新共通ShellとAPI clientへの適応、frontendチェック。
3. 認証担当: 最新認証・モジュール契約の監査、full app router回帰テスト。既存RBACを巻き戻さない。
4. 配置担当: 非公開Driveスナップショットの設定・Docker除外・rollback・容量契約を監査。

編集先は分離した `job-copy-react-join` checkout。共有gurnardでcheckout/rebaseをしない。求人・応募の既存同期、架電の業務ロジック、競合調査を変更しない。新しいストア・認証方式は追加しない。React画面を `/app/job-copy` へ追加し、Pythonレビューサーバーを本番として組み込まない。

## 完了条件

最新アプリの共通ナビからReact画面へ到達し、未ログイン／権限不足を既存方式で拒否できる。既存API clientのGET/POST/upload契約を維持する。Rust・Reactの適切な単体／統合／E2Eを実施し、実データ36求人317応募と45画像参照を保持する。秘密・顧客データをGitやDockerへ入れない。本番反映とGoogleログイン検証は統合窓口に引き継ぎ、未検証の本番成功を報告しない。
