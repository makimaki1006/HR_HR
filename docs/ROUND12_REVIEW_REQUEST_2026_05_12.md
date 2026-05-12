# Round 12 修正レビュー依頼書

**日付**: 2026-05-12
**対象**: 採用コンサルレポート PDF (本番 https://hr-hw.onrender.com)
**Round**: 12 (媒体分析タブ・PDF 章単位での chart 機能性復元)
**依頼者**: Claude (実装担当)
**レビュアー**: ユーザー (および任意の第三者)

---

## 1. レビュー依頼の目的

Round 11 までで「ヒストグラムだけ部分改善 + 他 chart の重大な機能不全が放置」という事態が明らかになり、Round 12 で **媒体分析タブの主要 chart (K9-K12 対象範囲) を機能単位で検証 + 確定バグ修正** を実施した。本依頼は、Round 12 修正が本当に「**chart として機能している**」かを **データ critical 5 基準** で再確認するための第三者レビュー依頼。

(注: N1-N4 系の横展開で発見した新規バグ候補 (helpers.rs xAxis.formatter / ヒートマップ ECharts 化 / industry_mismatch.rs の 4 象限「図」不在 / 軸名 cross-check test 不在) は本 Round の scope 外、Round 13 持ち越し)

---

## 2. 修正対象と本番反映状況

### 2.1 commit

実装関連:
- `4678a1d` fix(survey-pdf): restore chart functionality (K9/K10/K11/K12/K17, Round 12 P0)
- `b4c58c0` test+fix(survey): Round 12 Phase 1 unit tests + K3 alert phrasing + minor cleanup

docs:
- `c442990` docs(round12): functional review (Round 12 監査プロセス + 計画)
- `3e74ee5` docs(round12): post-implementation retrospective + review request (本依頼書 + §6 振り返り追記)

origin/main 同期済。

### 2.2 本番反映状況

**実装 commit (`4678a1d` / `b4c58c0` / `c442990`) の本番反映確認済**: 2026-05-12 16:09 の本番 PDF 再生成 + PNG 視覚レビューで Round 12 修正の反映を確認。

本依頼書 commit (`3e74ee5`) はレビュアー閲覧用 docs のみで本番動作には影響しない。

### 2.3 本番 PDF アクセス手順

- URL: `https://hr-hw.onrender.com` → 媒体分析タブ → 「採用コンサルレポート PDF」ボタン
- **認証**: 管理済みの環境変数 / secrets を参照 (`$env:E2E_EMAIL` / `$env:E2E_PASS`、実値は `.env` 経由)
  - **注**: 本 docs に credentials 平文を記載しない (P0 修正済、§5 残課題 Secret rotate も参照)
- ローカル取得済 PDF (Round 12 検証用): `out/round8_p0_1_prod/mi_prod.pdf` (28 pages, mtime 2026-05-12 16:09)
  - 注: パス名は `_round8_p0_1_prod` だが Round 12 検証も同 spec を流用。spec OUT_DIR を Round 12 専用に分岐する改善は Round 13 想定。

---

## 3. レビュー対象 (合格判定対象 5 件 + 残課題確認対象 1 件)

### 3.0 区分

| 区分 | 対象 | レビュー観点 |
|---|---|---|
| **合格判定対象** | K9 / K10 / K11 / K12 / K3 | 「修正完了」として合格できるか判定 |
| **残課題確認対象** | K17 (部分修正のみ) | 関数厳格化で OK、caller 側修正を次 Round で扱う合意の確認 |

### 3.1 合格判定対象 5 件

### K9 ✅ 人口ピラミッド X 軸 (page 10)

**修正前**: X 軸目盛が `-1,000,000` `-1,500,000` のマイナス表記
**修正後**: `xAxis.axisLabel.formatter` で `Math.abs(v).toLocaleString()` 適用 → 絶対値表示
**確認方法**: page 10 の図 D-1 「年齢階級別 人口ピラミッド」の X 軸目盛が正の値か

### K10 ✅ 求職者心理章 chart 化 (page 19)

**修正前**: 図 11-1「給与レンジ認知」が **数字 3-4 個並びのみ** (chart のフリした text card)、図 11-2「経験者プレミアム」も同様
**修正後**:
- 図 11-1: 横棒グラフ (平均上限 / 求職者期待値 / 平均下限) + 3 KPI box + レンジ幅ドーナツ (狭い/中程度/広い)
- 図 11-2: 縦棒グラフ (経験者求人 vs 未経験可求人 平均給与比較)

**確認方法**: page 19 で chart が描画されているか、数値ラベルが chart 内に表示されているか

### K11 ✅ IQR boxplot 化 (page 4)

**修正前**: 「図 3-1 IQR シェード」が CSS div の横長スライダー風
**修正後**: ECharts boxplot type で 5 数要約 (min/Q1/中央値/Q3/max) を視覚化、補助 IQR シェードは残置 (CSS、印刷簡易表現)
**確認方法**: page 4 の図 3-1 が boxplot 形状か

### K12 ✅ ヒートマップセル縦サイズ (page 15)

**修正前**: `.heatmap-cell` に縦サイズ指定なしで thumbnail 化
**修正後**: `min-height: 36px` + flex 中央寄せ
**確認方法**: page 15 の都道府県別 求人件数ヒートマップ Top 10 のセルが読みやすい高さか
**残課題**: Top 10 のみで 47 県全体マップではない (P2 改善余地)

### K3 ✅ 最賃割れアラート文言 (page 16)

**修正前**: 「N 県で最賃を下回る傾向。差が 50 円未満 (要確認): M 県」 → ユーザーが「N 県は下回るのに M 県は 0?」と矛盾と読んだ
**修正後**: 「**最賃割れ: X 県**」と「**最賃以上だが余裕 50 円未満 (時給ベース): Y 県** (or 該当なし)」を並列明示
**確認方法**: page 16 の最低賃金比較セクション冒頭の severity badge + 文言が新形式か

---

### 3.2 残課題確認対象 1 件 (K17)

### K17 ⚠️ 採用ターゲット粒度 (page 10、**部分修正のみ**、合格判定対象外)

**修正前**: `is_target_age` 関数が 5 歳階級 (25-29/30-34/35-39/40-44) と 10 歳階級 (20-29/30-39/40-49) の両方を `true` 扱い → 説明文「25-44 ターゲット」と実集計「20-49」の粒度ズレ
**修正後 (関数のみ)**: `is_target_age` を 5 歳階級専用に厳格化
**未対応 (caller 側)**: 10 歳階級データで `is_target_age` 全件 false の場合、KPI ラベル「25-44 採用ターゲット層」を非表示にする caller ロジックは未実装
**確認方法**: page 10 で「25-44 採用ターゲット層 N 人 (M%)」KPI が依然表示されている (= caller 修正は次 Round 持ち越し)
**レビュー観点**: 関数のみ修正で次 Round 持ち越しという判断に合意できるか

---

## 4. レビュー基準 (data critical 5 基準)

各 chart に対し以下を確認:

| 基準 | 旧基準 (浅い) | 新基準 (data critical) |
|---|---|---|
| 1 データ完全性 | 「chart が描画されている」 | 全データが描画されているか (女性 bar / 全年齢階級 / 全自治体 etc.) |
| 2 軸表示形式 | 「軸ラベルがある」 | 軸表示形式が正しい (絶対値、対数明記、外れ値除外) |
| 3 中央軸 | 「凡例がある」 | 中央 0 軸が明示されているか (左右対比 chart で起点が分かる) |
| 4 粒度整合 | 「形状が chart らしい」 | データ粒度と説明文の整合 (例: 10 歳刻み vs 「25-44」ターゲット) |
| 5 業界標準 | (考慮なし) | 業界標準フォーマット遵守 (国勢調査=5 歳階級 etc.) |

加えて:
- 「数値が読めるか?」(data label / 軸目盛 / 単位)
- 「PDF static で機能するか?」(tooltip 依存していない)
- 「採用判断に使える具体的判断材料を提供するか?」

---

## 5. 既知の残課題 (本依頼後の Round 13+ 想定)

| # | 内容 | 優先度 |
|---|---|---|
| K17 caller | 10 歳階級データで「25-44 KPI」ラベルを非表示にする caller 修正 | P1 |
| K1 | aggregator マスタ整合性 (川崎市=神奈川 等のガード) | P1 |
| K2 | 表 7-1 列順 (市区町村→都道府県) UX | P2 |
| K6 | 母集団レンジ重複行 (SQL 層) | P2 |
| K16 | 散布図 X 軸 splitNumber / axisPointer | P2 |
| K7/K8 | 人口ピラミッド DB データ調査 (`v2_external_population_pyramid`) | DB |
| N1 | 全ヒストグラム xAxis.formatter なし (helpers.rs) | P2 |
| N2 | 図 6-1「ヒートマップ」が CSS grid (ECharts ではない、名称不一致) | P2 |
| N3 | industry_mismatch.rs に「4 象限図」がなく表のみ | P2 |
| N4 | 全 chart で xAxis.name vs caption 自動 cross-check test 不在 | P2 |
| ヒートマップ Top 10 | 47 県全体マップ化 | P2 |
| 未経験 0 件の chart | 「データなし」注記表示 | P2 (軽微) |
| **Secret rotate** | e-Stat APP_ID / E2E credentials の **public リポ過去 commit への平文露出** (Round 11 F セキュリティ audit で発見、その時は「優先度下げ」判断したが、未対応の間は public 履歴に永続。本依頼書では平文を記載しないよう修正済 (P0)。**実 token revoke + rotation が未完了の場合は P0**) | **P0 (未対応の場合)** |

---

## 6. レビュー観点・質問事項

1. **K9/K10/K11/K12/K3 の 5 件は本当に「chart として機能している」か?** PNG 視覚レビュー + 採用判断に使える状態か
2. **K17 部分修正の妥当性**: 関数のみ修正 + caller 別 Round 持ち越しで合意できるか、それとも今すぐ caller も含めて完了させるべきか
3. **boxplot (K11) の見せ方**: 横向き 1 系列で十分か、雇用形態別 / 経験者 vs 未経験別 で複数 boxplot 並べる方が情報量大か
4. **求職者心理 (K10) の chart 種別選定**: 横棒 + 縦棒 + ドーナツ の組合せで十分か、別の chart 種別 (箱ひげ / scatter / ヒストグラム) が適切か
5. **ヒートマップ (K12)** の Top 10 制約は今回 scope 外で OK か、47 県全表示を Round 13 に
6. **アンチパターン test 設計** の妥当性: 確定バグを「現状 PASS で固定 → 修正後 FAIL → 反転して PASS」の 3 段階 lifecycle で運用する設計、レビュー基準として有用か

---

## 7. 参考資料

- `docs/ROUND12_FUNCTIONAL_REVIEW_2026_05_12.md` — 検証プロセス + K1-K17 確定診断 + 振り返り
- 本番 PDF: `out/round8_p0_1_prod/mi_prod.pdf` (28 pages)
- PNG 視覚レビュー出力: `out/_round12_review/page_*.png` (13 pages)
- Phase 1+2 unit test: 169 件 (helpers / region / wage / market_intelligence / market_tightness / mod.rs::round12_integration_tests)
- cargo test --lib: 1478 passed / 0 failed / 2 ignored

---

## 8. レビュー後の期待アクション

レビュー結果が以下のいずれかになることを期待:

| 結果 | 次アクション |
|---|---|
| ✅ Round 12 修正で合意 | docs に「レビュー完了」マーカー追記 + 次 Round (P1 残課題 K17 caller / K1) 着手判断 |
| ⚠️ 要追加修正 | 指摘事項を priority 付きで明示 → 私が即時対応または Round 12.5 として別 commit |
| ❌ Round 12 修正そのものに問題あり | 根本見直し (chart 種別変更 / 関数設計やり直し) を Round 13 で着手 |

レビューをお願いします。
