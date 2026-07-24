# 求人票生成パイプライン (jobgen) — HR_HR 統合ドキュメント

- 作成: 2026-07-25
- 対象: HR_HR に統合した求人票生成機能 (jobgen) と、姉妹機能キーワード需要ビューア (media_engine) の運用・保守
- 移植元: `job_media_engine_rs` (Hubspot プロジェクト)
- 引き継ぎ資料 (移植時の正本):
  - `Hubspot\docs\wbs_outputs\求人媒体選定エンジン\求人票生成部_引き継ぎ_2026-07-24.md`
  - `Hubspot\docs\wbs_outputs\求人媒体選定エンジン\検索エンジン部_引き継ぎ_2026-07-24.md`
- 設計正本: `Hubspot\docs\job_creation_media_engine_generation_pipeline_v1_2026-07-24.md`

---

## 1. 概要

顧客の求人原文 (自由テキスト / URL / CSV / Excel / PDF / HTML) から、戦略提案 (市場分析・ペルソナ・キャッチコピー・画像案+生成プロンプト・スマホ原稿・A/B助言) と HRハッカー84列原稿を生成するパイプライン。

**ハルシネーション対策の設計思想**: 検証は全てコードで行い (引用実在照合 / 数値照合[E]2段 / NGワード50ルール / 文字数上限)、LLM に検証させない。検証不合格の項目は空欄+人間レビュー行きにする (それっぽい値で埋めない)。

Gemini 呼び出しは 1求人あたり約 14〜17回 (工程分割+ペルソナ別+工程⑤bの+1回)。プロセス共通の 12回/分レートリミッタを解説資料・商談準備・CSV抽出と共有するため、通し実行は 1.5〜2分程度。UI に工程進捗が表示される。

## 2. 統合の経緯 (コミット履歴)

| commit | 日付 | 内容 |
|---|---|---|
| fca2bef | 2026-07-24 | キーワード需要ビューア (media_engine) 統合。SerpApi 枠/キャッシュを Turso 一次に |
| 8759e53 | 2026-07-24 | Docker 修正: builder 段に keywords.html、runtime 段に重心CSV |
| e4116a7 | 2026-07-24 | 求人票生成 (job_gen) 統合。知識データ埋め込み・二段認証・preserve_order 有効化 |
| 78e7339 | 2026-07-24 | 工程⑤b 追加: ディレクション→画像生成AI用プロンプト変換 (+1コール) |
| 97b2691 | 2026-07-25 | ⑤b 強化: 全要素固定のセクション構造・曖昧語禁止・ペルソナのペイン接地・appeal_core |

## 3. アーキテクチャ

### 3.1 モジュール構成

```
src/job_gen/
  mod.rs          モジュール宣言 + 工程①〜⑧対応表
  types.rs        共有型 (FACT_KEYS 8キー・ExtractedFacts・facts_to_text)
  fact_extract.rs 工程① 事実抽出 + 引用実在チェック
  strategy.rs     工程②③④⑤⑤b⑥⑧ プロンプトビルダ + responseSchema
  validate.rs     数値照合[E] 2段 (数値存在 + 数値単位ペア)
  hrhacker.rs     工程⑦ 84列 (HRHACKER_COLUMNS は Python 正本と 84/84 一致)
  ng_words.rs     NGワード検出 (50ルール24グループ、NFKC正規化+元位置復元)
  knowledge.rs    職種別知識ローダ (152職種+汎用6、最長マッチ)
  inputs.rs       入力6形式正規化 + SSRFガード + サイズ上限
  handlers.rs     axum ハンドラ (main.rs から抽出、HR_HR 適応)
```

### 3.2 知識データ (バイナリ埋め込み)

`assets/` の 3ファイルを `include_str!` でコンパイル時同梱。Render のディスク非永続に依存しない。

| ファイル | 内容 |
|---|---|
| `ng_words.json` | NGワード50ルール (正本: Sheets「求人系」NGワードタブ。法令根拠つき) |
| `ng_context_exclusions.json` | 適法定型文の文脈除外6ルール。**掲載点検専用、生成ゲートには適用しない** |
| `knowledge_bundle.json` | 職種別知識 152職種+汎用6 (約1MB)。index + シート結合 |

- env `KNOWLEDGE_DIR` を設定すると再ビルドなしでファイルシステム側に差し替え可能
- **知識更新手順**: 正本 (Hubspot 側 `data/job_creation_media_engine/knowledge/`) を更新 → `build_knowledge_assets.py` で assets/ 再生成 → HR_HR の `assets/` にコピー → リビルド

### 3.3 依存の要点

- **serde_json `preserve_order` を有効化** (84列 row の列順維持に必須。HRHACKER_COLUMNS の挿入順 = UI の CSV 列順)。有効化時に全2429既存テストの回帰なしを実測確認済み
- `calamine` (Excel)、`pdf-extract` (PDF、optional feature `pdf`、default 有効。壊れる環境では feature を外すと PDF 入力のみ明示エラー)
- Gemini は `crate::media_engine::gemini::generate_json` 経由 (12回/分共通リミッタ)
- Dockerfile: builder 段に `COPY static/jobgen.html` と `COPY assets/` が必須 (include_str! のため)

## 4. HTTP API 契約

パスは UI (`static/jobgen.html`) と掲載点検スクリプト (`hr_listing_audit.py`) が依存するため**変更しないこと**。

| POST | 工程 | リクエスト | レスポンス (status:"ok" 時) |
|---|---|---|---|
| /api/jobgen/normalize | 入力 | {kind, text?, url?, data_base64?} | {jobs:[{title_hint, source_text}]} |
| /api/jobgen/extract | ① | {source_text} | {facts, facts_text} |
| /api/jobgen/analyze | ② | {source_text, job_title} | {category, knowledge_used, analysis} |
| /api/jobgen/personas | ③ | {source_text, analysis, count?(3-5)} | {personas:[...]} |
| /api/jobgen/copy | ④ | {persona, analysis} | {copies, ng_violations, review_required} |
| /api/jobgen/images | ⑤ | {personas} | {directions:[{persona_label, direction}]} |
| /api/jobgen/image_prompts | ⑤b | {directions, personas?} | {prompts:[{persona_label, appeal_core, prompt, negative_prompt, aspect_ratio}]} |
| /api/jobgen/mobile | ⑥ | {persona, facts_text} | {lines, ng_violations, review_required} |
| /api/jobgen/hrhacker | ⑦ | {source_text, facts, strategy_hint?} | {attempts, row(84列・正典順), generated_fields, review_required_fields, unsupported_numbers} |
| /api/jobgen/ab | ⑧ | {summary} | {steps:[{metric, action}]} |
| /api/jobgen/ng_check | 点検 | {items:[{key, text}]} (最大50,000件) | {checked, flagged, results} |

エラー時は常に `{status:"error", message}` (HTTP 200)。

## 5. 認証 (二段構え)

ユーザー決定 (2026-07-24): **生成系もトークン併用**。

```
リクエスト → API_AUTH_TOKEN 一致? ─ yes → 通す (スクリプト向け)
                    │ no / 未設定
                    ▼
             CSRF 検査 + HR_HR セッション認証 (ブラウザ向け)
```

- トークンは `X-Api-Token` ヘッダまたは `Authorization: Bearer <token>`
- env `API_AUTH_TOKEN` 未設定ならトークン経路は存在しない (セッション認証のみで動く)
- 実装: `src/job_gen/handlers.rs::jobgen_auth_middleware` (lib.rs の jobgen_routes に適用)

## 6. 工程⑤b: 画像生成プロンプト化 (2026-07-24 追加 / 07-25 強化)

工程⑤のディレクション (人間向け演出案) を、画像生成AIにそのまま貼れる日本語プロンプトへ変換する。全ペルソナ一括 1コール。

**ユーザー決定**: 日本語プロンプトのみ / 2段階化 (+1コール) / ネガティブ+構図カメラ+アスペクト比+撮影指示書兼用。

**強化 (97b2691) の設計原則「解釈の余地を残さない」**:
- 出力プロンプトは 7セクション構造を強制: 【被写体】【場面・背景】【小道具】【構図・カメラ】【光・色調】【スタイル】【禁止事項】
- 人数固定 (「合計N名のみ。これ以外の人物は写さない」) を必須化。服装は色まで、表情・視線・手の位置・画面内配置・レンズ焦点距離相当まで指定
- 曖昧語 (「適度に」「自然な感じ」「〜など」) を禁止
- 【禁止事項】を本文に埋め込む (ネガティブ欄を持たない ChatGPT/Gemini 系でも抑制が効く)
- ペルソナの不満・ペインを変換コールに渡し、`appeal_core` (この画像が一瞬で感じさせるべきこと) を先に言語化させてから全要素をそれに接地させる
- temperature 0.4 (演出の発散は工程⑤側で済んでいるため指示遵守優先)

**検証実績 (2026-07-25 本番)**: 介護・ドライバー・製造検査・機械加工の4ペルソナで全チェック通過。ペインの裏返しが禁止事項に反映されることを確認 (例: 車中泊がペインのドライバー → 禁止事項「長距離トラックの過酷なイメージ、車中泊の道具」)。

## 7. 環境変数

| 変数 | 必須 | 用途 |
|---|---|---|
| `GEMINI_API_KEY` | 必須 | 全生成工程 (Render 設定済み) |
| `GEMINI_MODEL` | 任意 | 既定 gemini-3.5-flash-lite |
| `API_AUTH_TOKEN` | 推奨 | スクリプト向けトークン認証の有効化 (§5)。**未設定時はセッション認証のみ** |
| `KNOWLEDGE_DIR` | 任意 | 知識データのファイルシステム差し替え (§3.2) |

## 8. 検証記録 (2026-07-24〜25 実測)

- `cargo test --lib`: 2522件パス / 0失敗 (job_gen 93件含む)
- 84列パリティ: Python 正本 `hrhacker_columns.py` と 84/84 列名・順序一致 (照合スクリプトで機械確認)
- preserve_order 有効化の回帰: 既存2429テスト全パス
- 本番: /jobgen 200、未認証 POST 303 拒否、ng_check 実検出 (「女性歓迎」→性別差別表現、「35歳以下」→年齢差別、「経験不問」→クリーン)、⑤b 実出力の構造チェック全通過

## 9. 関連システム

- **HRハッカー掲載点検** `hr_listing_audit.py` (Python、Hubspot 側): NGワード判定を `/api/jobgen/ng_check` に委譲する設計。本番に向ける場合は `--endpoint https://hr-hw.onrender.com/api/jobgen/ng_check` + `API_AUTH_TOKEN`
- **キーワード需要ビューア (media_engine)**: 同アーク統合の姉妹機能。`/keywords-ui` + `/api/{keywords,suggest,regions,forecast,serp,cluster}`。SerpApi 月次カウンタ/キャッシュは Turso 一次 (`jme_serpapi_quota` / `jme_serpapi_cache`)、1レポート=1クエリ厳守、月240枠。Google Ads 資格情報が無い環境ではナビリンク自体が非表示

## 10. 落とし穴 (運用時の注意)

1. `jobgen.html` / `keywords.html` / `assets/` は **include_str! 埋め込み**。編集後はリビルドしないと反映されない
2. `ng_context_exclusions.json` は掲載点検専用。生成側 NGゲート (copy/mobile/hrhacker) に適用しないこと
3. Rust ビルドは OneDrive 配下で行わない (Temp 側 `hr_hr_build` で実施)。push 前に必ずフルワークスペース `cargo build --release` (gen_vrt_fixtures の教訓)
4. Dockerfile の COPY 漏れに注意: 新しい include_str! 参照や実行時読み込みファイルを足したら builder/runtime 段への COPY を確認

## 11. 残課題 (移植と独立、引き継ぎ資料 §8 と同期)

- NGワードシート notes 由来語彙 (主夫/パパ/看護婦等) の人手キュレーション追加
- URL / Excel 入力の実物 E2E (ユニットのみ)
- 戦略成果物の納品形式 (現状は画面表示+84列CSVダウンロードのみ)
- 職種マッチの限界 (「伊達事務所」→事務職に最長マッチ等) は UI の職種名編集欄で人が補正する運用
- 生成系フル通し (①〜⑧) の本番実行は未実施 (工程単位では検証済み)
