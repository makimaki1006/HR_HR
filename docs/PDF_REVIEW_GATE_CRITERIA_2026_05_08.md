# PDF レビューゲート基準 (PDF_REVIEW_GATE_CRITERIA)

**策定日**: 2026-05-08
**適用対象**: V2 (HelloWork) MarketIntelligence variant の Print/PDF 納品
**作業ディレクトリ**: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`
**前提 commit**: `ad6488d` (Round 6 完了時点)

本ドキュメントは「E2E PASS = 納品 OK」と誤判定する事故を防止するため、Print/PDF 納品判定の必須要件を再定義する。**自動検査の通過は十分条件ではない。人間目視による全ページ確認を必須とする。**

策定起点: 2026-05-06〜2026-05-08 Round 6 で発生した以下の誤判定事例:
- **注釈統合誤判断 (旧 page 19 相当 / 現在は `data-mi-section="annotations"` セクション)**: 文字数削減効果を期待した修正が実際には別ターゲットだった (commit `9a616f9`)
- **PDF 余白計測誤認**: フッター margin box を本文最下端として誤計測し「4mm → 12mm 改善」を誤認 (`docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md`)
- **ヒストグラム軸ラベル重なり (`[data-chart="histogram"]` / 旧 page 5・6 相当)**: 自動検査では検出されず、目視でのみ確認可能 (`ROUND6_PRINT_PDF_DELIVERY_QUALITY.md` §5 P2-C)

> **注**: 章構成変更により page 番号は流動的である。本 docs では以後、レビュー対象を page 番号で固定指定せず、**図番号 / 見出し / セクション名 / `data-mi-section` / `[data-chart="*"]` selector** ベースで指定する (§1.6 参照)。

---

## 1. 必須要件 (人間目視) — 1 件でも欠落すれば「納品 OK」判定不可

これらは自動検査で代替できない。レビュアーは以下を全て満たした上でのみ「納品可」を提案する。

### 1.1 PDF 全ページの高解像度 PNG 確認 (DPI 150 以上)

- 全ページを **DPI 150 以上**で PNG 化し、1 枚ずつ目視で確認すること。
- 縮小一覧 (contact sheet) のみでの判定は禁止。文字つぶれ / グラフ歪み / 軸ラベル重なりが contact sheet では検出できない。
- ページ送りで「変更箇所だけ」を見るのも禁止。周辺ページへの副作用 (空白 page 発生 / 章割れ / chart 再描画失敗) を見落とすため。
- 推奨ツール例: PyMuPDF (`page.get_pixmap(dpi=150)`) または Playwright `page.screenshot({ fullPage: true })` の高解像度モード。

### 1.2 問題ページごとの記録

各問題は以下のフォーマットで記録すること。記録なしの「目視 OK」は無効とする。

| 項目 | 内容 |
|---|---|
| セクション識別子 | `data-mi-section` 値 / `[data-chart="*"]` selector / 章見出し / 図番号 (例: `data-mi-section="market-intelligence"`、`[data-chart="histogram"]`、「給与ヒストグラム (図 3-2 〜 3-5)」、「採用市場逼迫度レーダー (図 M-2 / 図 RC-1)」、「給与×件数散布図 (図 5-1)」、`data-mi-section="ranking"` 配信地域ランキング) |
| page 番号 (参考) | レビュー時点の実 PDF 上の page 番号 (章構成変更で変動するため**補助情報**としてのみ記載。固定参照に使わない) |
| カテゴリ | レイアウト / グラフ / 文言 / 余白 / 重複 / その他 |
| 重大度 | P0 (納品阻害) / P1 (磨き込み) / P2 (将来課題) |
| 詳細 | 1-2 行で具体的な事象 |
| 想定原因 | 1 行で想定 (推測の場合は「要検証」と明記) |

**page 番号運用ルール**:
- 指示書 / docs 側で「page X を確認」と固定指定しない。`data-mi-section` 値 / `[data-chart="*"]` selector / 図番号 / 章見出しのいずれかで指定する。
- レビュー実施時に観測された page 番号は記録テーブルの「page 番号 (参考)」欄に残してよいが、後続ラウンドの照合キーとしては使用しない。
- 章構成変更で page 番号が変動した場合も、selector / 図番号は安定するためレビュー指示書を改訂しなくてよい。

重大度の運用基準:

- **P0**: 営業に持ち出して読めない / 説明できない / 取引先に出せない (例: Hard NG 用語混入、自治体重複、グラフ見切れ、内部 fallback 文言)
- **P1**: 読めるが磨き込みが必要 (例: 軸ラベル微妙な重なり、章割り再考、ヒストグラム間隔調整)
- **P2**: 将来的な拡張 / 体感品質の追加改善

### 1.3 GAS 参考デザインとの差分確認 (該当する場合)

- 参考デザイン (GAS / 既存テンプレート) が指定されているセクションは、レンダリング後の PDF と並べて差分を確認する。
- 章順 / 見出し階層 / 色運用 / 表組 / 凡例位置の不一致を P0 / P1 で記録。
- 参考デザインがない場合は本項目を「N/A」と明記して飛ばす。

### 1.4 セグメント分析網羅性確認

下記セグメントが**漏れなく**レポートに含まれていることを目視で確認する。

- 地域 (都道府県 / 市区町村 / 政令市区)
- 職種 (job_seeker_data / 求人区分)
- 性別 (該当データがある場合)
- 年齢 (年齢階層)
- 業界 (industry / 産業大分類)
- 給与帯 (賃金分布 / 賃金センタイル)

漏れがある場合は P0 として記録し、原因 (データ欠落 / レンダリング欠落 / 仕様外) を切り分ける。

### 1.5 採用コンサル目線での「説明に使える」判定

- 各章を読み、**そのページ単独で何を主張しているか**を 1 文で言い切れるか確認する。
- 言い切れないページは P1 として記録し、注釈追加 / 文言整理を提案する。
- 「相関のみで因果と読まれるリスク」がある記述は P0 として修正必須 (`feedback_correlation_not_causation.md` 準拠)。
- 中立表現規約 (BtoB レポートでの「劣位」「集中」「縮小」評価語禁止 / `feedback_neutral_expression_for_targets.md`) に違反する文言は P0。

### 1.6 page 番号でレビューしない原則 / selector ベース探索手順

**原則**: 章構成変更で page 番号がずれるため、レビュー指示書 / docs では page 番号を固定参照に使わない。代わりに以下のキーで指定する。

| 指定キー | 用途 | 例 |
|---|---|---|
| `data-mi-section="<id>"` | MI variant 内の section root 識別 | `cover` / `market-intelligence` / `annotations` / `ranking` / `print-summary` |
| `[data-chart="<type>"]` | チャート要素の種別識別 | `histogram` / `radar` / `scatter` / `bar` |
| 図番号 (図 X-Y) | 文書内の図の固定 ID | 図 3-2 〜 3-5 (給与ヒストグラム) / 図 M-2 / 図 RC-1 (採用市場逼迫度レーダー) / 図 5-1 (給与×件数散布図) |
| 章見出し | 章単位の参照 | 「第 1 章 表紙」「給与ヒストグラム」「採用市場逼迫度レーダー」「給与×件数散布図」「配信地域ランキング」 |

**過去の固定 page 番号からの置換マトリクス** (Round 2 章構成変更時点):

| 旧記載 | 新指定 |
|---|---|
| page 1 表紙 | `data-mi-section="cover"` または「第 1 章 表紙」 |
| page 5 / page 6 ヒストグラム | `[data-chart="histogram"]` または「給与ヒストグラム (図 3-2 〜 3-5)」 |
| page 8 レーダー | `[data-chart="radar"]` または「採用市場逼迫度レーダー (図 M-2 / 図 RC-1)」 |
| page 14 散布図 | `[data-chart="scatter"]` または「給与×件数散布図 (図 5-1)」 |
| page 16 MI | `data-mi-section="market-intelligence"` |
| page 25 配信地域ランキング | `data-mi-section="ranking"` |

**selector ベース探索フロー** (該当 page を特定したい場合):

```
1. PDF からテキスト抽出
   - 例: PyMuPDF page.get_text("text") を全 page 走査
2. 図タイトル / セクションタイトルを grep
   - "図 5-1"、"給与×件数散布図"、"配信地域ランキング" 等で検索
   - HTML 由来の場合は data-mi-section / data-chart 属性を生成 HTML 側で grep
3. ヒットした page 番号を取得 (レビュー時点の参考値)
4. その page を DPI 150 以上で PNG 化 → 目視
5. 記録テーブル §1.2 のフォーマットで起票
   - 「セクション識別子」を主キー、「page 番号 (参考)」は補助
```

**章構成変更時の指示書改訂ルール**:
- 章構成 / page 順序を変更したラウンドでは、レビュー指示書側の selector / 図番号 / 見出しが実 DOM / 実 PDF と一致しているかを確認する。
- selector / 図番号 / 見出しが変わらない限り、page 番号変動だけでは指示書を改訂しなくてよい。
- selector / 図番号 / 見出しが変わった場合のみ、本 §1.6 の置換マトリクスに追記して指示書を更新する。

---

## 2. 必須要件 (自動検査) — 必要条件であり十分条件ではない

これらは Hard NG が即時検出できる項目に限定する。**全項目 PASS でも §1 の人間目視を省略してはならない。**

### 2.1 PDF page count

- 期待 page count レンジ (例: 30 ± 2 pages) を事前に定義し、外れる場合は要因調査。
- ただし page count だけで「内容妥当」と判断するのは禁止 (空白ページ混入時も page count は満たし得る)。

### 2.2 Hard NG 13 用語 0 件

- 検査対象用語 (例): `target_count` / `推定人数` / `想定人数` / `母集団人数` / その他 `tests/no_forbidden_terms` 定義の 13 用語。
- 検査方法: `cargo test --test no_forbidden_terms` PASS、かつ生成 PDF テキスト抽出に対しても grep 0 件。
- 用語リスト本体は本ドキュメントに転記しない (重複管理回避)。`tests/no_forbidden_terms` を Single Source of Truth とする。

### 2.3 MI 5 マーカー存在

下記 5 マーカーが MarketIntelligence variant PDF に全て出現することを確認:

- `mi-print-summary`
- `mi-print-annotations`
- `mi-parent-ward-ranking` (政令市区データ非空時のみ実体出力。fixture 由来 0 件の場合は除外判定)
- `mi-rank-table`
- hero bar (Card 1/2/3)

注: `mi-parent-ward-ranking` の 0 件判定は `POST_RELEASE_MONITORING_CHECKLIST.md` §1 注記と整合させる。

### 2.4 `data-mi-section` 存在

- MarketIntelligence variant root に `data-mi-section="market-intelligence"` が出力されていること。
- Full / Public / default variant には**出現しないこと** (variant_isolation 維持)。

### 2.5 chart count

- 期待 chart 数を事前に定義し、ECharts 初期化が完了した chart 数と一致することを確認。
- canvas 存在のみでは不十分 (`feedback_e2e_chart_verification.md` 準拠)。`echarts.getInstanceByDom` 経由で初期化済みを確認すること。

### 2.6 table count

- 期待 table 数 (例: ranking table / cost table / demographic table 等) と一致を確認。
- table 内の自治体重複は本検査では検出できない (§1.4 の人間目視で判定)。

### 2.7 right / bottom margin (フッター除外で計測)

- 右余白 / 下余白の計測対象は**本文 block の y1 / x1**。`@page` margin box 内のフッター文字列 (例: `Page X / N 株式会社...`) を含めて計測することは禁止。
- 計測方法は `docs/PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` §10 補足の擬似コード (FOOTER_PATTERNS 除外) を参照。
- 期待値: right_margin 36.9-39.2pt 維持 (Round 6 確定値)、bottom_margin > 12mm (本文最下端基準)。
- 反例: PyMuPDF `blocks` の生 max y1 を使うと margin box 内フッター下端 (= page_h - 11.2pt 付近) を拾い、CSS `@page` margin の改善が「反映なし」と誤判定される (Round 6 で実発生)。

### 2.8 variant_isolation 維持

- `Full` / `Public` variant の HTML / PDF に Step 5 (resident) マーカーが**含まれない**ことを確認。
- 検証コマンド例: `curl -s "<URL>/<variant path>" | grep -c 'data-mi-section="market-intelligence"'` が `0` であること (POST_RELEASE_MONITORING_CHECKLIST.md §1 と整合)。

---

## 3. 補助要件 (環境/契約)

§1 §2 とは別に、リリース環境/契約として下記を満たすこと。これら単独で「納品 OK」とは判定しない。

- E2E PASS (本番反映確認 / Render cold start 60s 対応 / `feedback_render_cold_start_timeout.md` 準拠)
- `cargo test --lib` PASS
- `cargo test --test no_forbidden_terms` PASS
- selector が `data-mi-section` 経由で実 DOM とヒットする (`docs/SPEC_SELECTOR_AUDIT_2026_05_08.md` §1.2)
- 部分コミット時は依存チェーン (include_str! / pub mod / 可視性) を確認 (`feedback_partial_commit_verify.md` 準拠)

---

## 4. 「納品 OK」判定フロー

下記のステップを順に実施する。途中で P0 が検出されたら以降のステップを止め、修正ラウンドに戻る。

```
1. cargo / E2E が PASS                    ← §3 補助要件
   ↓
2. PDF 生成成功 (期待 page count レンジ内)  ← §2.1
   ↓
3. 全ページ PNG 化 + 目視 (DPI 150 以上)    ← §1.1 §1.2
   ↓
4. 問題を P0/P1/P2 分類 (§1.2 フォーマット記録)
   ↓
5. P0 が 0 件 → §1.3 §1.4 §1.5 を実施     ← セグメント網羅性 / コンサル目線判定
   ↓
6. §2 自動検査の全項目 PASS を確認        ← Hard NG / マーカー / margin
   ↓
7. P0 残 0 件 + P1 が許容範囲 (磨き込みレベル)
   → 「納品 OK」候補へ
   ↓
8. 当番リーダー / ユーザー承認後、納品完了

途中で P0 検出時:
   → 即時修正ラウンドへ。「P0 残あり」のまま納品提案するのは禁止。
```

承認権限の補足:
- 「納品 OK」判定はレビュアー単独で確定しない。当番リーダー / ユーザーの明示的承認を経ること。
- `POST_RELEASE_MONITORING_CHECKLIST.md` §4 rollback 判断基準に該当する事象が見つかった場合は、納品判定を保留し rollback 判断フローに合流させる。

---

## 5. アンチパターン (明示的に NG)

下記の判定手順は**いずれも単独で「納品 OK」とする根拠にならない**。Round 6 で実際に発生した誤判定パターンを含む。

### 5.1 `cargo test PASS だけで完了報告`

- ライブラリテストは Hard NG 用語 / 不変条件の一部しか検証しない。
- 紙面の見え方 (グラフ歪み / ラベル重なり / レイアウト崩れ) は cargo test では検出不可能。
- 該当教訓: `feedback_llm_visual_review.md`

### 5.2 `PDF を contact sheet (縮小一覧) のみで判定`

- 縮小表示では文字つぶれ / 軸ラベル重なり / 表組ズレが視認不可。
- 必ず DPI 150 以上のページ単位 PNG で確認する (§1.1)。

### 5.3 `機械検査だけで「OK」`

- §2 の自動検査全 PASS でも、人間目視 (§1) を省略してはならない。
- 機械検査は「Hard NG が無い」「マーカーが揃っている」までしか保証しない。
- 該当教訓: `feedback_test_data_validation.md`、`feedback_reverse_proof_tests.md`

### 5.4 `変更箇所だけ目視 (周辺ページの副作用見落とし)`

- 例: 注釈統合 (commit `9a616f9`) で「対象セクション (`data-mi-section="annotations"`) が空白になっていないか」だけ確認し、前後章 (隣接セクション) の章割れを見落とすパターン。
- §1.1 の通り「全ページ目視」が必須。

### 5.5 `text block 抽出だけで page 内容判定` (Round 6 教訓: 注釈統合セクション誤判断)

- text block の文字数 / snippet 数だけで「内容が改善された」と判定するのは禁止。
- 例: 「文字数が減ったから紙面効率改善」と推定 → 実際は別ターゲットで効果未実現 (`ROUND6_PRINT_PDF_DELIVERY_QUALITY.md` §6.4)。
- 必ず実 PDF の見え方を目視で確認する。

### 5.6 `PyMuPDF blocks に @page footer 含む計測` (Round 6 教訓: 4mm → 12mm 改善誤認)

- `page.get_text('blocks')` の生出力には `@page @bottom-*` margin box 内のフッター文字列が含まれる。
- これを本文最下端として max y1 で拾うと、CSS `@page margin` の変更が常に「反映なし」と誤判定される (フッター下端余白は margin box 仕様で約 4mm 一定)。
- §2.7 の通り FOOTER_PATTERNS 除外で計測すること。
- 該当ドキュメント: `PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` §2.2 §10

---

## 6. 既存ドキュメントとの関係

本ドキュメントは下記既存 docs と矛盾しない形で「納品判定」観点を追加するものとする。既存 docs は本ラウンドでは改変しない。

| 既存 docs | 関係 |
|---|---|
| `POST_RELEASE_MONITORING_CHECKLIST.md` | リリース後の継続監視。本 docs はリリース直前の納品判定を扱う。§4 rollback 判断基準は本 docs §4 から参照。 |
| `ROUND6_PRINT_PDF_DELIVERY_QUALITY.md` | Round 6 運用記録。本 docs §5 アンチパターンの起点教訓を提供。 |
| `PDF_BOTTOM_MARGIN_ROOT_CAUSE_INVESTIGATION.md` | 余白計測誤認の調査記録。本 docs §2.7 §5.6 で参照。 |
| `MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md` | Print/PDF P1 設計仕様。本 docs §1.4 セグメント網羅の前提。 |
| `SPEC_SELECTOR_AUDIT_2026_05_08.md` | selector 監査。本 docs §3 で参照。 |

---

## 7. 改訂履歴

| 日付 | 改訂内容 |
|---|---|
| 2026-05-08 | 初版策定 (Round 6 教訓を反映)。 |
| 2026-05-08 | §1.2 記録フォーマットを selector / 図番号ベースに更新。§1.6 「page 番号でレビューしない原則 / selector ベース探索手順」追加。Round 2 章構成変更で page 番号がずれた事象を反映。 |
