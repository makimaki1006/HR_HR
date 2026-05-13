# B 領域監査: データ整合性 / 数値ロジック

監査日: 2026-05-13
対象: `src/handlers/survey/report_html/`, `src/handlers/company/`, `src/handlers/insight/`, `scripts/`
読み取り専用、コード変更・build なし。

## サマリー

良好な点:
- `invariant_tests.rs` (907 行) に 10 種ドメイン不変条件 test が整備済 (失業率 <=100%, レーダースコア 0-100, 6 マトリクス sum<=pool, 規模構成比 100%±0.1, 雇用形態 dedup test)。
- `vacancy_rate` は内部「0-1」レシオで一貫保持、表示時 `*100.0` で % 化 (`market_tightness.rs:755, 837, 1091, 1216`)。単位混在なし。
- `employee_delta_1y` は実装側でパーセント単位として `> 10.0`, `> 5.0`, `< -5.0` で運用 (`company/fetch.rs:814, 846, 848`)。2026-04-30 100倍ずれ事故の再発防護あり。
- 「産業計」相当 (`AS`, `AR`, `CR`) は SQL で `NOT IN` 除外 (`scripts/.../subtab5_phase4_7.rs:286, 301`)。
- 比率分母の 0 除算は概ね `if total > 0 { ... }` で保護 (demographics, market_intelligence, region, regional_compare, wage)。
- Rust 側 dedup test は雇用形態を key に含む構造 (`survey/upload.rs:1031-1040`)。

## 検出事項

### P1-1: 規模帯平均成長率の 0 件混入バイアス
- 場所: `src/handlers/survey/report_html/salesnow.rs:876-884`
- 内容: 「規模帯横断ほぼ均一」takeaway の条件が `growth_spread < 2.0 && total >= 5` のみで、`large/mid/small_count` が 0 でないことを検証していない。`(large_growth + mid_growth + small_growth) / 3.0` の分母は常に 3 固定。
- 実害シナリオ: `large_count=0, mid_count=5, small_count=5, large_growth=0.0 (default), mid_growth=2.0, small_growth=2.0` → 算出 avg=1.33%。実態 (large 不在) では平均 2.0% であるべき。レポートに「規模を横断して人員推移はほぼ均一 (差 1.3pt 以内、平均 +1.3%)」と表示され、unbiased な数値ではない。
- 不変条件 test: 当該箇所に対する不変条件 test 不在 (invariant5 は `large_count=0` 時に「縮小傾向」takeaway が出ないことのみ検証、平均算出の歪みは未検証)。
- 修正方針: 計算前に `count > 0` の帯のみで平均する、または「ほぼ均一」を 3 帯すべて充足時に限定する。

### P1-2: CSV vs HW 正社員構成比の用語非対称
- 場所: `src/handlers/survey/report_html/executive_summary.rs:548-565`
- 内容: CSV 側 `fulltime_count` は `emp_type.contains("正社員") || emp_type.contains("正職員")` の寛容判定。HW 側 `hw_ft` は `emp_group == "正社員"` の厳密一致。
- 実害シナリオ: アップロード CSV に「正職員」レコードが混入すると、csv_rate には算入されるが hw_rate からは除外され、diff が CSV 側に下駄。閾値 15.0pt 判定 (line 573) で誤検出/見逃しが発生。CLAUDE.md V1/V2 分離ルールで HW は「正社員」が正式とされているが、CSV 側は両方を受容しているため対称性が崩れている。
- 不変条件 test: 用語一貫性 test 不在。
- 修正方針: CSV/HW 両側で同じ用語セットに揃える、または HW 側でも 「正社員」「正職員」両方を集計する。

### P2-1: 散布図回帰線 NaN/Infinity 未検証
- 場所: `src/handlers/survey/report_html/scatter.rs:97-104`
- 内容: `y1 = (reg.slope * x_min_yen + reg.intercept) / 10_000.0` の結果を `is_finite()` チェックなしで `json!` に渡す。slope = NaN や Infinity の場合 serde_json は null/特殊文字列としてシリアライズし、ECharts 側で line 描画が壊れる、または例外を吐く可能性。
- 実害シナリオ: 入力 filter 後 6 点以上 (line 60) を確保しても、x の分散が極端に小さい場合 `agg.regression_min_max` の slope が `f64::INFINITY` になりうる。`agg` 側の計算経路に NaN ガードがあるかは別途 A 領域 (集計層) で要確認。
- 不変条件 test: scatter の y1/y2 有限性 test 不在。
- 修正方針: `if y1.is_finite() && y2.is_finite()` で markLine 追加を条件分岐する。

### P2-2: regional_compare top_industry の SQL 経路依存
- 場所: `src/handlers/survey/report_html/regional_compare.rs:315-329`
- 内容: `ext_industry_employees` 全行で total を計算し最大行を抽出。SQL 側 (`subtab5_phase4_7.rs:fetch_industry_structure`) で「産業計」相当の `AS/AR/CR` が `NOT IN` 除外されているため現状は安全。ただし当該 Vec が他経路 (test fixture, 別ハンドラ流用) で「産業計」を含む状態で渡された場合、合計が二重カウントされ top_industry % が半分になる。
- 実害シナリオ: 現本番経路は安全。将来別 fetch 経由で同 Vec を再利用すると顕在化。
- 不変条件 test: `invariant7_industry_filter_section_renders_with_data` (line 638) は section レンダリングのみ検証、産業計除外は test 不能と明記 (line 632-635)。
- 修正方針: `regional_compare.rs:315` で `industry_code` に対する除外フィルタを Rust 層にも追加するか、industry_name に「産業計/サービス計」が含まれる行を除外する防御層を入れる。

### P2-3: salesnow 「平均」計算の 0 値混入 (パターン 2/3)
- 場所: `src/handlers/survey/report_html/salesnow.rs:763, 777`
- 内容: パターン 2 (全規模マイナス) とパターン 3 (全規模プラス) は前段で `large/mid/small_count > 0` を確認済のため安全。P1-1 と異なり実害なし。確認のため記載。

### P2-4: コメントと実装の単位表記混在 (情報的)
- 場所: `src/handlers/company/fetch.rs:643, 904`
- 内容: コメント「`employee_delta_1y > +0.10`」とあるが実装は `> 10.0` (line 814)。実体はパーセント単位で統一されているのでバグではないが、コメントが混乱を生む。
- 修正方針: コメントを `> +10.0%` に修正。

## 不変条件 test カバレッジギャップ

既存 invariant_tests.rs でカバーされていない不変条件:

1. 規模帯平均算出時の 0 件混入バイアス (P1-1 関連)
2. CSV/HW 用語対称性 (P1-2 関連)
3. 回帰線 slope/intercept の有限性 (P2-1 関連)
4. 散布図軸範囲 (`compute_axis_range`) の妥当性 (min < max, 範囲 > 0)
5. cascade 集計時の `avg_salary_min > 0.0` フィルタ後の最終平均の妥当範囲 (上限・下限)

## 結論

A 領域 (UI/レンダリング) ではない数値ロジック層は、不変条件 test の整備により 2026-04 系事故 (unemployment 380%, employee_delta 100x) の再発リスクは大幅に低減されている。残課題は (a) 規模帯横断平均の 0 件混入バイアス (P1-1) と (b) CSV/HW 用語非対称 (P1-2) の 2 件で、どちらも特定の DB 値で確実に発火する。NaN/Infinity 防御 (P2-1) は防御的検証の追加レベル。
