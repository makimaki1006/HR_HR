# V2 HW Dashboard 全面監査 統合レポート

**統合日**: 2026-04-25
**統合担当**: 親セッション (5専門チーム監査の同期統合)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**手法**: 並列監査 (5チーム × worktree隔離 × read-only) → 統合分析
**所要**: 約 12 分 (各チーム並列実行) + 統合フェーズ
**ユーザー指示**: "編集をする必要は無い、まずは調査、ultrathinkingで専門のチームを作ってプランを作成して調査してください"

---

## 1. 監査体制と分担

| チーム | 役割 | レイヤー | 報告書 | 行数 |
|---|---|---|---|---|
| α | User-facing 品質 | L1 ペルソナ整合 / L5 UX / L6 誠実性 / 用語一貫性 | `team_alpha_userfacing.md` | 260 |
| β | System Integrity | L2 IA / L3 データパイプライン / 契約整合 | `team_beta_system.md` | 234 |
| γ | Domain Logic Quality | L4 38 insight / 給与統計 / 雇用形態 / 8 パネル | `team_gamma_domain.md` | 500 |
| δ | Code Health | L7 dead code / テスト / 依存 / ファイル肥大 | `team_delta_codehealth.md` | 486 |
| ε | Persona Walkthrough | 3 ペルソナ × 3 シナリオ = 9 通しテスト | `team_epsilon_walkthrough.md` | 295 |

**合計分析行数**: 約 1,775 行 (この統合レポート除く)

---

## 2. レイヤー別総合スコア

| レイヤー | スコア | 評価 | 主因 |
|---|---|---|---|
| **L1 ペルソナ整合** | 4.0 / 5 | 良 | 9 タブ全て明確なペルソナ対応。条件診断 vs 採用診断のみ機能重複 |
| **L2 IA / 動線** | 2.5 / 5 | 要改善 | insight/trend がナビ非表示・タブ呼称 4 重ブレ・47県横断ビュー欠如 |
| **L3 データパイプライン** | 4.0 / 5 | 良 | 4 系統 graceful degradation・panic 0 件・19 個 INDEX 自動付与 |
| **L4 ドメインロジック** | 3.0 / 5 | 注意 | 重大バグ疑い 3 件 (MF-1 10x ズレ / vacancy_rate 概念混乱 / posting_change muni粒度詐称) |
| **L5 UX / 認知負荷** | 3.0 / 5 | 普通 | 配色・モバイル対応は良。詳細分析の 30+ ビュー / 専門用語密度が高い |
| **L6 誠実性** | 4.5 / 5 | 優 | phrase_validator 機構・HW限定注記・相関≠因果が網羅的。市場概況のみ抜け |
| **L7 コード健全性** | 3.5 / 5 | 注意 | 依存・テスト整備は良 (647件)。ファイル肥大 (4,594行) / dead code / 文書乖離 |
| **総合** | **3.5 / 5** | **B (Good with Critical Fixes Needed)** | バックエンド堅牢 / UI 動線とドメイン精度に弱点 |

---

## 3. ペルソナ達成度 (Team ε より)

| ペルソナ | 主シナリオ達成度 | 主因 |
|---|---|---|
| A 採用コンサル | 2.7 / 5 | 統合 PDF が無く顧客提出資料に 8 枚スクリーンショット必要 |
| B HW 利用企業 人事 | 3.7 / 5 | 条件診断 + 採用診断 + 企業検索の 3 タブ往復が必要 |
| C 採用市場リサーチャー | 3.7 / 5 | 47 県横断比較なし・地域カルテ citycode 必須で都道府県カルテ作れず |
| **平均** | **3.4 / 5** | フロントエンド動線改善の ROI が極めて高い |

---

## 4. 横断発見: クリティカル課題 (Top 10)

### 🔴 P0: 即対応 (1日以内)

#### #1 jobmap 契約 Mismatch #1 + #4 が 2 日経過しても未修正
**チーム**: β / δ 一致
**証拠**: `src/handlers/jobmap/handlers.rs:399` (name キー欠落)、`src/handlers/jobmap/company_markers.rs:128` (municipality キー欠落)
**影響**: 地図タブのツールチップで `undefined: 0人` 表示が継続
**修正**: backend に各 1 行追加 (`"name": m_name,` / `"municipality": muni,`) + `global_contract_audit_test.rs:438-468` の `#[ignore]` 解除
**工数**: 5 分
**注**: `#[ignore]` のため CI 警告も出ず silent failure 状態

#### #2 MF-1 医師密度 単位 10倍 ズレ疑い
**チーム**: γ
**証拠**: `engine.rs:1565` `NATIONAL_PHYSICIANS_PER_10K = 27.0` だが計算式は `physicians / total_pop * 10000.0` で「人/1万人」を出力 → 比較対象 27 (実態は人/10万人) で割ると ratio が 1/10
**影響**: 全市区町村で「全国 10% 未満 → 医師不足」誤発火の可能性大
**修正**: physicians テーブル単位確認 → 定数 2.7 に修正 OR コメント修正
**工数**: 検証 30 分 + 修正 5 分

#### #3 vacancy_rate の概念混乱 (HS-1 / HS-4 / FC-4 / RC-3 / IN-1 / balance タブに波及)
**チーム**: γ
**証拠**: CLAUDE.md L223 で `v2_vacancy_rate` = 「recruitment_reason_code=1 (欠員補充) の比率」と定義。労働経済の欠員率 (=未充足求人/常用労働者数) ではない
**影響**: 「欠員率 30%」という表示が労働経済統計の欠員率と誤読される
**修正**: ETL 段階で命名変更 (`v2_replacement_demand_share`) OR UI 表記を「欠員補充求人比率」に統一
**工数**: 中 (全 vacancy 言及箇所の grep + 表記統一)

### 🔴 P0: 1 週間以内

#### #4 posting_change_3m/1y_pct を市区町村単位と詐称
**チーム**: γ
**証拠**: `hw_enrichment.rs:108-128` で都道府県単位 fetch → `HwAreaEnrichment` (key=`{prefecture}:{municipality}`) に流し込み
**影響**: 媒体分析で「○○市の3ヶ月人員推移 +20%」と表示するが実態は都道府県全体の値
**修正**: ts_turso_counts に muni 粒度追加 OR UI に「※都道府県全体の値」明記
**工数**: UI 注記なら小、データ拡張なら中

#### #5 CTAS fallback 14 箇所の戻し作業 (2026-05-01 期日)
**チーム**: β / δ
**証拠**: `flow.rs:88,112,137,163,196,213,229,238,266,281` + `flow_context.rs:51,138,208`
**影響**: 5/1 Turso リセット後に GROUP BY 動的集計から CTAS テーブル参照に戻す必要。忘れると本番性能 10x 劣化継続
**修正**: `docs/flow_ctas_restore.md` 通り CTAS 投入 → コメントマーカーで grep 一発置換 → 逆証明 SQL で総和一致確認
**工数**: 1 日 (CTAS 投入 + Rust 戻し + 逆証明)

### 🟡 P1: 2 週間以内

#### #6 ナビ動線: insight / trend が「詳細分析」内に隠蔽
**チーム**: α / ε / B/C/A 全ペルソナ
**証拠**: `dashboard_inline.html:70-89` のタブ群に `/tab/insight`, `/tab/trend` リンクなし。`analysis/handlers.rs:50-53` のグループ切替経由のみ
**影響**: 主要分析機能の発見性が極めて低い (リサーチャー / コンサル両方で指摘)
**修正**: 上位ナビに「総合診断」「トレンド」を追加 (HTML 修正のみ)
**工数**: 小

#### #7 タブ呼称の 4 重ブレ
**チーム**: α
**証拠**: 「求人検索」(UI) / 「競合調査」(コメント) / 「企業調査」(H2) / 「企業分析」(別タブ company H2) / 関数名 `competitive` / URL `/tab/competitive`
**影響**: ユーザーが「求人検索」をクリック → 「企業調査」表示 → 別タブ「企業検索」では「企業分析」と表示 → 4 単語混乱
**修正**: 一語に統一 (推奨: 「求人検索」)
**工数**: 中 (文字列置換)

#### #8 雇用形態分類の二重定義 (survey vs recruitment_diag)
**チーム**: γ
**証拠**:
- `aggregator.rs:678-682`: 契約/業務委託 → 正社員グループ (月給ベース)
- `mod.rs:74-81`: 契約社員 → 「その他」展開先
**影響**: 業務委託の月額報酬が正社員月給中央値を歪める可能性。同じ UI でタブ毎に違う集計値
**修正**: `emp_classifier.rs` 単一モジュール作成 → 両所から呼出
**工数**: 中

#### #9 統合 PDF レポート機能の不在
**チーム**: ε / α / A・B ペルソナ
**証拠**: Insight=xlsx/JSON/HTML、Survey=HTML、採用診断=なし、地域カルテ=印刷HTML、詳細分析=PNG のみ。タブ越え統合 PDF が存在しない
**影響**: コンサル A の「クライアント提出資料 1 本にならない」決定打
**修正**: 採用診断 + 媒体分析 + 地域カルテを 1 PDF に統合する `/report/integrated` エンドポイント
**工数**: 大

#### #10 ルート CLAUDE.md (2026-03-14) が 40+ 日未更新
**チーム**: δ
**証拠**: `recruitment_diag` (4-23 事故対応中核機能), `insight`, `survey`, `region`, `trend`, `SalesNow` 統合をいずれも記載せず
**影響**: マスターリファレンスとして機能不全。新規参入者の事故再発リスク
**修正**: 現実装ベースで 9 タブ + Round 1-3 数値 + memory feedback 参照リンクで再構成
**工数**: 中

---

## 5. その他重要発見 (P2 推奨)

### L4 ドメインロジック (Team γ 由来、計 10 件)

| ID | 内容 | 重大度 |
|---|---|---|
| M-2 | SW-F02 vs SW-F05 同時発火: holiday_day_ratio 1.5 以上で「人材不足」と「観光ポテンシャル」併発 | 🟡 |
| M-3 | SW-F03 vs SW-F08 中間沈黙: daynight_ratio 0.8-1.3 の市区町村は両方発火しない | 🟢 |
| M-7 | IN-1 発火条件反転疑い: `!(0.05..=0.3).contains(&mw_share)` は通常範囲 (10-15%) で発火しない | 🟡 |
| M-8 | SW-F06 仕様乖離: 仕様は AND 条件 (人流回復 AND 求人遅延) だが実装は人流のみ | 🟡 |
| - | engine.rs 既存 22 パターン (HS/FC/RC/AP/CZ/CF) が `assert_valid_phrase` 未呼出 → 「不足しています」「全国中央値に到達できます」など断定表現混在 | 🟡 |
| - | LS-1 の「未マッチ層が約{失業者数}人」は失業者全員が HW 未マッチであるかの誤誘導 | 🟡 |
| - | Panel 1 採用難度: 分母が「Agoop 平日昼滞在人口」のため観光地・繁華街でスコア低下 → 「穴場」誤判定 | 🟡 |
| - | Panel 5 emp_type フィルタが UI 値そのまま (`expand_employment_type` 未経由) → ヒット 0 で「データ不足」誤表示 | 🟡 |
| - | RC-2 給与差閾値 ±10000円/-20000円が職種別給与水準を考慮しない (介護と IT で同閾値) | 🟢 |
| - | SW-F04 / SW-F10 が `None` 返却 (未実装プレースホルダ) | 🟢 |

### L7 コード健全性 (Team δ 由来)

- **dead route 6 件** (`/tab/overview, /tab/balance, /tab/workstyle, /tab/demographics, /tab/trend, /tab/insight`): 旧 dashboard.html 用、UI 到達不可。ただし `/api/insight/report*` は外部呼び出し経路として活きている可能性
- **`render_section_hw_enrichment_legacy_unused` 147 行**: 削除タイミング明示なし
- **`src/handlers/diagnostic.rs.bak` 37KB**: `.gitignore` に `*.bak` なく誤コミットリスク
- **環境変数 4 個 が config.rs 外**: `TURSO_EXTERNAL_URL/TOKEN`, `SALESNOW_TURSO_URL/TOKEN` が `main.rs` で直接 `env::var` 読出し (`AppConfig` 統合違反)
- **ファイル肥大**: `analysis/render.rs` 4,594 行 / `survey/report_html.rs` 3,912 行 / `analysis/fetch.rs` 1,897 行
- **ワークスペース汚染**: 138 個 PNG, `_*_mock.csv`, `_sec_tmp/`, `*.bak` が `.gitignore` 未登録
- **`format!` 濫用 329 箇所**: `write!(html, ...)` で代替可
- **`unwrap()` 256 箇所**: production 経路 (handlers/insight 26, survey 25, recruitment_diag 23, region/karte 13) で要精査

### L1/L5/L6 User-facing (Team α 由来)

- **`templates/tabs/overview.html` が V1 求職者ダッシュボードの遺物**: `{{AVG_AGE}}=月給`, `{{MALE_COUNT}}=正社員数` の意味取り違え変数。誤使用すると即事故
- **市場概況タブの H2 直下に HW 限定 banner なし** (フッター注記のみ)
- **詳細分析 (`/tab/analysis`) に「相関≠因果」明示文言なし** (insight/karte/recruitment_diag/correlation は網羅的)
- **媒体分析の集計値に「IQR外れ値除外」UI 文言なし** (コードはやっているが UI 上に出ていない)
- **雇用形態セレクトの選択肢がタブ毎に異なる**: competitive=3種 / recruitment_diag=3種 / jobmap=4種

---

## 6. Keep / Fix / Rebuild 推奨

### Keep (維持すべきユニーク価値)
1. **HW + SalesNow + Agoop 人流 + e-Stat 外部統計の 1 画面結合**: 他に存在しない独自分析
2. **phrase_validator 機構**: 「100%/必ず/確実に」を走時検証する誠実性メカニズム
3. **Insight 22+10 パターンの定型示唆**: So What を自動生成する仕組み
4. **graceful degradation 設計**: panic 0 件、Turso None でも空応答
5. **採用診断 8 パネル並列ロード** + **AppCache 3 層 (DashMap+TTL+max_entries)**
6. **HW 限定性の透明性**: recruitment_diag/karte/insight/correlation で網羅的注記 (集約は必要だが姿勢は維持)
7. **同名市区町村の citycode 区別** (伊達市/府中市): バグなし、優秀

### Fix (修正で済む)
1. **P0 の 5 件 (#1〜#5)** + **P1 の 5 件 (#6〜#10)**: 上記 Section 4
2. **既存 22 patterns に phrase_validator 適用**: コード変更小、効果大
3. **タブ呼称統一**: 文字列置換で完了
4. **dead code 削除** + **`.gitignore` 強化** + **bak / mock CSV / PNG 整理**
5. **環境変数 4 個の `AppConfig` 統合**: テスト容易性向上

### Rebuild (要設計やり直し)
1. **`survey/report_html.rs` (3,912 行)**: PDF 設計仕様書 (2026-04-24) で全面再構成予定。section 単位分割
2. **`analysis/render.rs` (4,594 行)**: 6 サブタブ × 28 セクションを抱える単一ファイル。サブタブ単位で分割
3. **dead route 6 件 (`/tab/overview` 等) と V1 テンプレート (`templates/tabs/overview.html` 等)**: 外部 API 呼出有無確認後、削除 OR UI 復活
4. **統合 PDF レポート機能**: 新規実装。コンサル/営業の決定打になる差別化
5. **47 都道府県横断比較ビュー**: 新規実装。リサーチャー C の決定打

---

## 7. 推奨アクションシーケンス (1 週間)

### Day 1 (今日 2026-04-25 残り)
1. **#1 jobmap Mismatch #1, #4 修正** (5 分): backend 2 行追加 + `#[ignore]` 解除
2. **#2 MF-1 単位検証** (30 分): physicians テーブル実値確認 → 定数修正

### Day 2-3
3. **#3 vacancy_rate 表記統一** (半日): 全 vacancy 言及 grep → UI ラベル統一
4. **#4 posting_change muni 粒度詐称対応** (UI 注記版で半日)
5. **dead code 削除** (#5 P1 後段、半日): `_legacy_unused`、`diagnostic.rs.bak`、`.gitignore` 強化

### Day 4-5
6. **#6 ナビ昇格** (insight/trend を上位ナビへ、半日)
7. **#7 タブ呼称統一** (1 日): 「求人検索」に統一
8. **#10 ルート CLAUDE.md 再構成** (1 日): 9 タブ + Round 1-3 数値 + memory 参照

### Day 6 (5/1 直前)
9. **#5 CTAS 戻し作業** (1 日): Turso リセット待ち → CTAS 投入 → Rust 戻し → 逆証明

### Day 7 (バッファ)
10. **既存 22 patterns に phrase_validator 適用** + **engine.rs 残課題棚卸し**

---

## 8. 残存リスクと未確認領域

### 本監査で検証できなかった項目
| 項目 | 理由 | 推奨対応 |
|---|---|---|
| `cargo build / test` 実行ログ | sandbox 制約 | ユーザー手動 `cargo build --release 2>&1 \| grep warning` |
| `physicians` テーブルの実単位 | DB アクセスなし | SQL 1 行で確認 (例: `SELECT total_doctors, total_pop FROM xxx LIMIT 5`) |
| `v2_vacancy_rate` の実値スケール (0-1 vs 0-100) | 同上 | SQL 1 行で確認 |
| `postings.municipality` が区単位/市単位 | ETL 仕様未読 | `hellowork_etl.py` 確認 |
| Turso 側 INDEX 設計 | Rust コード非可視 | Python ETL の `CREATE INDEX` 文確認 |
| labor_flow N+1 リスク | 実行計画未取得 | `cargo run` + `tracing` で SQL 計測 |
| モバイル / A11y 実機検証 | ブラウザ未起動 | Playwright で 9 タブ実機検証 |
| `templates/dashboard.html` (V1) と V1 dead route の生死 | grep 結果のみ | 外部 API 呼出ログから判定 |
| `/api/insight/report*` 外部利用者の有無 | アクセスログ未取得 | nginx/render ログ確認 |
| `static/js/` 配下の dead code | スコープ外 | フロントエンド健康度監査 |

### 監査の限界 (5 チーム共通)
- すべて **コード読取のみ**。実際の動作確認 / 実データ妥当性検証はユーザー手動が必要
- worktree 隔離のため互いの暫定発見を共有できない (各チーム独立判断 → 統合で照合)
- agent タスク 12 分制限で深掘り未完: 政令指定都市の集計粒度・SalesNow 精度・閾値 P25/P50/P75 の元値分布など

---

## 9. 監査の総評

**強み**:
- バックエンド堅牢度・データ統合のユニーク性・誠実性メカニズムは業界水準を超えている
- テスト 647 件 + bug marker 方式で「気づき」を仕組化する文化は先進的
- HW 限定性 / 相関≠因果 / phrase_validator の 3 重防御は他の SaaS にない誠実性

**弱み**:
- **コードと UI の乖離**: バックエンドは強固だが、UI 動線・PDF 統合・横断比較で価値が伝わっていない
- **「気づき仕組み」が #[ignore] で動かない**: bug marker は CI で silent。1 日経過しても修正されない
- **既存 22 patterns の phrase_validator 未適用**: 後発 16 patterns は適用済み。同一エンジン内で防御の差がある
- **ドキュメント乖離**: ルート CLAUDE.md が 40+ 日前。マスターリファレンス機能不全

**全体評価**: **B (Good with Critical Fixes Needed)** — 3.5 / 5
- 🔴 P0 5 件 (1 週間以内) と 🟡 P1 5 件 (2 週間以内) を完遂すれば **A (Excellent)** に到達可能
- ペルソナ達成度を 3.4 → 4.0 に引上げる鍵は **動線改善** (P1 #6, #7) と **統合 PDF** (P1 #9)

---

## 10. 個別レポート参照

詳細な根拠 (file:line) は以下の各レポート参照:

- [`team_alpha_userfacing.md`](./team_alpha_userfacing.md) — 260 行、19 ファイル参照
- [`team_beta_system.md`](./team_beta_system.md) — 234 行、24 ファイル参照
- [`team_gamma_domain.md`](./team_gamma_domain.md) — 500 行、38 patterns 全数検証 + 14 ファイル参照
- [`team_delta_codehealth.md`](./team_delta_codehealth.md) — 486 行、リポ全体スキャン
- [`team_epsilon_walkthrough.md`](./team_epsilon_walkthrough.md) — 295 行、3 ペルソナ × 3 シナリオ

---

**監査終了**: 2026-04-25 23:55 JST
**次回推奨**: P0 完遂後 (5/2 以降) に再監査で改善確認
