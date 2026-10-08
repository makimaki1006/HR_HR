# 求人文面管理 (/app/job-copy) — 開発セッション引き継ぎ (2026-10-08)

このセッションは **求人文面管理だけ** を担当する。架電 CRM (`/app/crm`, `frontend/src/screens/crm/`, `src/crm/`) は別セッションが並行で開発中なので触らない。

## 1. 画面の目的 (ユーザーの言葉を要約)

AirWork と HRハッカーの媒体求人を CSV から取り込み、**給与条件・求人票の本文・画像の変更**を比較する。HubSpot の**応募獲得情報**、**掲載期間**、**課金**、**Indeed の市場データ (求人数など)** と同じ時間軸で突き合わせ、「なぜその期間に応募が来た/来なかったか」「応募数はいくつか」を比較分析できるようにする。

ユーザーの不満の出発点: 「今はこれらが網羅的に視認できない」「情報が乱立しすぎて本来の目的を果たせない」。

## 2. 守るルール (ユーザー指示)

- **画面に開発者用語を出さない**: 「版対応不明」「本文観測」「観測版」「ctk」「MOC」「snapshot」、HubSpot の内部プロパティ名などは禁止。利用者の言葉で書き、説明はツールチップへ。
- 情報を詰め込まない。1 画面目は「求人一覧 + タイムライン」。目的に関係の薄いものは格下げ・統合。
- 因果を言い切らない (「効果」「確実に」「必ず」「要因」を断定に使わない)。応募は HubSpot に記録されたものだけと明記。応募レコードは「件」で数える (「人」は使わない)。件数 n を必ず出す。
- 推測を事実として出さない。取得できないものは「未取得」「不明」。0 と書かない。
- **検証の規模**: 既定は「実装 + 1 周 (3 観点並列) + 修正」。重要な本番反映でも 2 周まで。5 周はユーザーが明示したときだけ。
- `main` への push = Render 本番に自動デプロイ。デプロイ後はサーバー再起動でログインが切れる。CRM セッションのデプロイとぶつからないよう、マージ前に `gh pr list` と `origin/main` の動きを確認する。

## 3. 今の状態 (本番 = main)

| PR | 内容 |
|---|---|
| #87 | タイムライン (掲載期間・給与・本文・画像・課金・応募・市場) と求人の横断比較。取込系は「データ取込」へ集約、逆検索は一覧見出しのボタンへ。住所はサーバー側で 都道府県+市区町村 に丸め、3 件未満の組み合わせは隠す。応募理由の自由記述はマスク |
| #88 | 応募理由の集計 (選択済み / キーワードで推定 / 分類できない)。タイムラインの「応募理由」レーン、横断比較の「多い応募理由」列。詳細は `claudedocs/JOB_COPY_REASONS_2026-10-08.md` |

決定済み・実装済みの扱い:
- 版の日付は **取得日** (媒体の変更日ではない)。変化は「取得日A〜取得日Bの間に変化」として出す。取得日当日・間の期間の応募は前後どちらにも入れない。7 日未満の期間は 1 日あたりで比べない。
- 画像は URL・中身 (bytes)・並び順・未取得を区別。失敗を「画像なし」「変更なし」にしない。
- **課金はダミー** (`dummyBilling.ts`、`DUMMY_BILLING_ENABLED`)。「仮の課金データ（ダミー）」と明示、合計・並べ替えに使わない、画面のチェック「仮の課金データを表示」で消せる。実データ (HRH の `cost_yen`、課金 CSV) が同じ日を覆えば実データが優先。
- 課金 CSV はブラウザ内だけで保持 (再読み込みで消える)。媒体 + 店舗ID/アカウントID + 求人ID の複合キーで照合、重複・期間重複は拒否。**実物の列はまだ不明** (ユーザーがスプレッドシート/CSV で提供予定)。
- Indeed 市場データ: `/api/job-copy/market` (ローカル SQLite `data/indeed_insights.db` の `insight_title_pref`、Turso ではない)。月次・都道府県・職種 126。**月 1 回更新される**ので終わりの月はデータから出す (固定しない)。求職者数そのものは無く閲覧者指標のみ。

## 4. 次にやること (優先順)

1. **掲載期間を HubSpot の実データに置き換える** (未着手・ユーザーに提案済み)
   - 求人 (Listing `0-420`) の `koukai_kaishi_nichiji` 公開開始日時 / `koukai_shuuryou_nichiji` 公開終了日時: 32,447 件、**HRハッカーのみ**。今のタイムラインは取得日から期間を推定している。
   - あわせて `baitai_genjoukyou_hrhakkaa` / `baitai_genjoukyou_airwork` 媒体原ステータス、`saishuu_csv_kenshutsu_bi` 最終CSV検出日 を表示候補に。
   - 値が最新の期間だけ (履歴ではない) の可能性が高い → 「媒体上の公開期間 (最新)」として扱い、過去の期間を推測しない。AirWork は取得日ベースのまま。
2. 課金 CSV の実物が来たら列の対応を合わせ、ダミーを外す (`DUMMY_BILLING_ENABLED = false`)。
3. 応募理由: 専用項目 (`ouboriyuukategori_hiaringu` 応募理由カテゴリ_ヒアリング 等) は 2026-10-08 時点で **0 件**。実際は 応募動機 (78)・転職理由 (69) に入っている。運用側に「カテゴリ」の入力をお願いする提案済み (回答待ち)。選択肢の内部値は日本語名そのまま、ただし「未設定」だけ `unset`。
4. 求人オブジェクトには使えそうで空の項目がある: 出稿予算・主要出稿の開始/終了・カイゼン日/メモ・応募獲得数_累計 (すべて 0 件)。

## 5. データの事実 (2026-10-08 に読み取りで確認)

- 応募 (`0-421`) 20,786 件。応募 → 求人 (Listing) にだけ関連し、取引・会社・コンタクトには直接つながらない。求人 → 取引 (1〜5 件)。
- 求人 (`0-420`) 40,208 件。`id_hrhakkaa` 32,511 / `id_airwork` 7,693。`job_copy_drive_manifest_v1` 28 件。
- 本番の実データ一覧は今 HRハッカーの 36 求人だけ (AirWork は表示用データに入る経路がまだ無い)。
- 取得・応募連携の契約は **PR #84 のブランチ** (`feat/job-copy-airwork-images`、別セッション所有) の `docs/architecture/job-copy-media-acquisition-handoff.md` と `job-copy-all-listings-handoff.md`。#84 のファイル (スクリプト・docs) は触らない。

## 6. 環境・道具

- 秘密値: `/Users/s_fujimaki/Downloads/env` (`HUBSPOT_ACCESS_TOKEN` 等)。値は表示しない。HubSpot は**読み取りのみ**、書き込みはユーザーの明示許可が要る。鍵は既存バッチ (sales-automation-api) と共有なので、Search は 1 秒 1 回以下・少回数に。429 が出たら止める。
- Rust のビルド置き場は **作業ごとに分ける** (`CARGO_TARGET_DIR=$PWD/target-private` など)。共有の置き場は別ブランチのテストを実行してしまう事故があった。
- E2E: `cargo build --bin rust_dashboard` → `python3 scripts/e2e/make_fixture_db.py /tmp/<dir>/hellowork.db` → `E2E_BIN=… E2E_FIXTURE_DB=… npx playwright test -c tests/e2e/pr/playwright.pr.config.ts`。求人管理は `tests/e2e/job-copy-server.config.ts` も。
- 既知の失敗: 競合調査の PDF ダウンロード E2E (503) は main でも失敗する既存問題。`frontend/src/screens/job-copy/timelineRound3.test.tsx` の「再試行後のフォーカス」テストはたまに落ちる (CI 再実行で通った)。直すなら別 PR で。
- 本番で実データを見るには https://hr-hw.onrender.com/app/job-copy (Google ログイン)。手元ではデモデータのみ (`?demo=1` / job-copy プレビュー)。

## 7. 参照

- `claudedocs/JOB_COPY_REASONS_2026-10-08.md` (応募理由)
- `docs/architecture/job-copy-functional-tabs-plan.md`、`job-copy-ux-self-audit.md` (2026-10-08 の変更記録あり)
- `docs/architecture/job-copy-reactions-performance-handoff.md` (応募理由・課金・市場の契約)
- PR #87 / #88 の本文 (変更点とテスト結果)
