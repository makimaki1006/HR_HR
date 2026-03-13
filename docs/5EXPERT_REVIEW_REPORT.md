# HelloWork Dashboard V2 - 5専門家レビュー統合レポート

**実施日**: 2026-03-14
**対象**: hellowork-deploy (Rust Axum + HTMX + ECharts)
**レビュアー**: データサイエンティスト / データアナリスト / HRコンサルタント / コーディングプロ / UI/UXプロ

---

## 統合優先度リスト（重複排除・優先度順）

### CRITICAL（即時対応推奨）

| # | 指摘 | 専門家 | ファイル | 対応工数 |
|---|------|--------|---------|---------|
| C-1 | **diagnostic.rs: emp_typeフィルタ未適用バグ** — `_emp_type`がアンダースコア付きでSQL WHERE句に使われていない。正社員月給25万をパート時給と比較してしまう | DS, HR | diagnostic.rs:209 | S (1h) |
| C-2 | **diagnostic.rs: 総合評価が給与のみ** — `overall_pct = salary_pct`で休日・賞与が反映されない。拡張版(0e9cb37)で**修正済み**（加重平均S/A/B/C/D化） | HR | diagnostic.rs:157 | **済** |
| C-3 | **analysis.rs: table_exists() SQLインジェクション** — `format!()`でテーブル名を直接埋め込み。diagnostic.rs版はパラメータバインドで安全 | Code | analysis.rs:771 | S (30m) |
| C-4 | **analysis.rs: 20+セクションが1ページ縦並び** — 情報過多でターゲットユーザー（人事担当者）には認知負荷が過大 | DA, HR, UX | analysis.rs | L (8h) |
| C-5 | **Python-Rust カラム名不一致（4箇所）** — テキスト温度計/企業戦略/独占指数/キーワードプロファイルのSQLカラム名がPython側テーブルと不一致。データ非表示の可能性 | DS | analysis.rs:284,910,931,889 | M (2h) |
| C-6 | **同期DB on async runtime** — r2d2同期プールがtokio workerスレッドをブロック。高負荷時にasyncタスク飢餓 | Code | db/local_sqlite.rs:49 | L (4h) |
| C-7 | **ARIA属性の欠落** — タブバーにrole="tablist"なし、select要素にlabel紐付けなし、フォーカス可視スタイル未定義 | UX | dashboard_inline.html | M (2h) |

### HIGH（次期スプリント推奨）

| # | 指摘 | 専門家 | ファイル | 対応工数 |
|---|------|--------|---------|---------|
| H-1 | **年収推定でボーナス未開示求人に平均ボーナスを適用** — 70%の求人に賞与なしなのに全体で年収=月給×15になる過大推定 | DS | compute_v2_salary.py:128 | M |
| H-2 | **3レベル集計で多重カウント** — 同一レコードが詳細/市区町村/都道府県の3キーに加算。都道府県集計が歪む | DS | compute_v2_salary.py:93 | L |
| H-3 | **充足困難度予測のデータリーケージ** — 学習データ全体で再学習し同データにpredict_proba。AUC 0.6066は過学習スコア | DS | compute_v2_prediction.py:204 | M |
| H-4 | **posting_activityとtalent_retentionが同一計算** — ベンチマーク6軸中2軸が実質同一。欠員率の影響が2/6=33% | DS | compute_v2_external.py:277 | M |
| H-5 | **市場分析タブにEChartsが1つもない** — 全てCSSバー。他タブとの視覚的一貫性が崩壊 | DA | analysis.rs | L |
| H-6 | **fetch関数のif/else 3分岐パターン25回重複** — 共通ビルダー化で大幅削減可能 | Code | analysis.rs | L |
| H-7 | **analysis.rs 2,018行のGod Object** — competitive/と同様にディレクトリ分割すべき | Code | analysis.rs | L |
| H-8 | **ヘルパー関数3重定義** — get_f64/get_i64/get_strが3ファイルで別シグネチャ | Code | overview/analysis/diagnostic | M |
| H-9 | **統計用語がそのまま表示** — Shannon指数/HHI/均等度は人事には意味不明 | HR | analysis.rs:498 | M |
| H-10 | **タブ間の情報導線がゼロ** — Tab1→Tab8等のクロスリンクがない | HR, UX | 全タブ | M |
| H-11 | **フィルタ操作時のローディングフィードバック不足** — 都道府県変更で3回fetchだが視覚的フィードバックなし | UX | dashboard_inline.html | S |
| H-12 | **ECharts凡例配置が不統一** — ドーナツの凡例がhorizontal/vertical/なしとバラバラ | DA | overview/workstyle/demographics | M |
| H-13 | **キャッシュエビクション戦略が弱い** — TTL内のエントリが大量にある場合メモリ際限なし成長 | Code | db/cache.rs:43 | M |
| H-14 | **Tailwind CSS CDN本番使用** — 公式非推奨。ビルド済みCSSに移行すべき | Code | templates/*.html | M |
| H-15 | **amenity_scoreでovertime=NULLを低残業扱い** — NULL=未開示≠低残業 | DS | compute_v2_market.py:178 | S |
| H-16 | **overview.rs給与帯分布にsalary_type='月給'フィルタなし** — 時給求人が混入し15万以下が膨張 | HR | overview.rs:474 | S |

### MEDIUM（計画的改善）

| # | 指摘 | 対応工数 |
|---|------|---------|
| M-1 | 最小サンプルサイズ基準が不統一（3/5/10/30） | S |
| M-2 | compensation_packageのパーセンタイル: 地域平均を個別求人分布で比較 | M |
| M-3 | HHI表示を0-10000スケールに変換 or 平文化 | S |
| M-4 | Gini係数を面積法（台形法）で数値安定性向上 | S |
| M-5 | テキスト温度計の0.01定数→epsilon 1e-8に | S |
| M-6 | ドーナツチャートにlabel追加（workstyle/demographics） | S |
| M-7 | 棒グラフのgrid設定統一 | S |
| M-8 | 産業×サイズクロスの凡例スクロール化 | S |
| M-9 | 求人地図のマーカークラスタリング導入 | M |
| M-10 | タブ切替時の自動スクロールトップ | S |
| M-11 | 印刷時のEChartsダークテーマ→白背景対応 | M |
| M-12 | 各タブに解釈ガイド/アクション示唆テキスト追加 | L |
| M-13 | 詳細検索にCSV/Excelエクスポート追加 | M |
| M-14 | MemoryStoreセッション→永続ストアへの移行検討 | M |
| M-15 | Dockerfile: `rust:latest`→バージョン固定 | S |
| M-16 | 標準偏差: N→N-1（不偏推定量）に修正 | S |

### LOW（技術的改善）

| # | 指摘 |
|---|------|
| L-1 | CSS変数とRustハードコード色の二重管理解消 |
| L-2 | postingmap.jsのイベントリスナーメモリリーク |
| L-3 | インデックス重複削除（idx_postings_prefecture等） |
| L-4 | テンプレートエンジン導入（askama/tera） |
| L-5 | Shannon正規化の最大値をln(k)に修正 |
| L-6 | 距離減衰パラメータの根拠コメント追加 |
| L-7 | download_db.shのgrep→jq化 |
| L-8 | Firefoxスクロールバースタイル対応 |

---

## 各専門家の総合所見

### データサイエンティスト
- 統計手法は基本的に教科書通りだが、**3レベル集計の多重カウント**が広範囲に存在
- **Python-Rust間のカラム名不一致**が4箇所あり、表示が壊れている可能性
- 充足困難度モデル(AUC 0.6066)はランダムよりわずかに良い程度。UIで「参考値」と明示すべき

### データアナリスト
- Tab1-4のECharts活用は完成度が高い
- **Tab6（市場分析）がEChartsゼロ**で全てCSSバー。最大の視覚的課題
- チャート設定の一貫性（凡例配置・grid設定・値ラベル）にばらつき

### HRコンサルタント
- 469K件のリアルタイム集計基盤は業界随一
- 欠員補充率・透明性スコア等の独自指標は高いコンサルティング価値
- **「見せっぱなし」が最大の弱点**。各指標の解釈ガイドとアクション示唆が必要
- emp_typeフィルタバグ(C-1)は診断結果の信頼性に直結

### コーディングプロ
- 小規模SaaS（数十ユーザー）として実用的に動作する水準
- **同期DB on async runtime**(C-6)が最重要。spawn_blocking導入を推奨
- analysis.rsの2,018行モノリシックファイルは分割が急務
- テストカバレッジ不足（analysis/diagnostic/balance/workstyle/demographicsにUT無し）

### UI/UXプロ
- 色覚多様性パレット（Okabe-Ito系）の定義は良い設計
- ECharts遅延初期化（htmx:afterSettle連携）は効率的
- **ARIA属性の欠落**とフォーカス可視スタイル未定義はアクセシビリティの基本要件
- 市場分析タブ・求人地図の認知負荷が過大。Progressive Disclosure推奨

---

## 推奨対応ロードマップ

### Sprint 1（即時: 1-2日）
- [x] C-2: 総合評価の加重平均化 → **0e9cb37で修正済み**
- [ ] C-1: emp_typeフィルタバグ修正
- [ ] C-3: table_exists()のパラメータバインド化
- [ ] C-5: Python-Rustカラム名不一致修正（4箇所）
- [ ] H-16: overview.rs給与帯のsalary_typeフィルタ追加

### Sprint 2（短期: 3-5日）
- [ ] C-7: ARIA属性追加 + フォーカス可視スタイル
- [ ] H-8: ヘルパー関数統一（db/helpers.rs）
- [ ] H-9: 統計用語の平文化（HHI→「産業集中度」等）
- [ ] H-11: フィルタ操作時のローディングフィードバック
- [ ] H-12: ECharts凡例配置統一
- [ ] H-15: overtime=NULL→0.5（不明）に修正

### Sprint 3（中期: 1-2週間）
- [ ] C-4: 市場分析タブのサブタブ/アコーディオン化
- [ ] C-6: spawn_blocking導入
- [ ] H-5: 市場分析タブへのECharts導入（レーダー/ゲージ）
- [ ] H-6+H-7: analysis.rsのリファクタリング（分割+共通化）
- [ ] H-10: タブ間クロスリンク追加

### Sprint 4（長期: 2-4週間）
- [ ] H-1-H-4: Python事前計算の統計手法改善
- [ ] H-14: Tailwind CSS CDN→ビルド済み化
- [ ] M-9: マーカークラスタリング
- [ ] M-12: 各タブの解釈ガイド追加
- [ ] M-13: CSV/Excelエクスポート
