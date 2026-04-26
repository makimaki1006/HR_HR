# Team γ: Domain Logic Audit Report

**監査対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**監査スコープ**: L4 Domain Logic Quality (採用市場分析の指標妥当性)
**監査日**: 2026-04-25
**監査者**: Team γ (read-only)
**注**: 出力先指定パス `docs/audit_2026_04_24/team_gamma_domain.md` への直書き不可のため、worktree 内 `docs/audit_2026_04_24/` に保存。手動コピー要。

---

## エグゼクティブサマリ

38 insight pattern (engine.rs 22 + engine_flow.rs 10 + StructuralContext 6) と関連する給与統計・雇用形態分類・8 採用診断パネル・媒体分析セクションを精査した。

**全体評価**: 「相関≠因果」の文末ヘッジ機構と RC-3↔GE-1 の cross-reference は適切に実装されているが、以下の **重大な指標誤り** が散見される。

| # | 問題 | 影響範囲 | 重大度 |
|---|------|----------|--------|
| 1 | `vacancy_rate` の意味誤認: 実装は「recruitment_reason_code=1 (欠員補充) の比率」であり、労働経済学の「欠員率 (= 未充足求人 / 全求人)」ではない | HS-1, HS-4, FC-4, RC-3, IN-1, balance タブ等 | 🔴 Critical |
| 2 | engine.rs 既存 22 パターン (HS/FC/RC/AP/CZ/CF) が `assert_valid_phrase` を呼ばず、断定表現が混在 | 22 パターン全体 | 🟡 Important |
| 3 | 雇用形態分類の不整合: `classify_emp_group_label` (survey) は 契約/業務委託 を「正社員」、`expand_employment_type` (diag) は 契約社員 を「その他」に分類 | survey vs recruitment_diag | 🟡 Important |
| 4 | `posting_change_3m_pct` / `_1y_pct` を市区町村単位の HwAreaEnrichment に格納するが、実体は都道府県単位データ | 媒体分析 HW 連携 | 🟡 Important |
| 5 | HS-4 の TEMP_LOW_THRESHOLD = 0.0: 閾値の物理的意味が文書化されていない | HS-4 | 🟡 Important |
| 6 | MF-1 の `NATIONAL_PHYSICIANS_PER_10K = 27.0` とコメント「2.7人/1万人」が 10倍食い違い | MF-1 | 🔴 要検証 |
| 7 | RC-2 の閾値 ±10000円/-20000円が職種別給与水準を考慮していない | RC-2 | 🟢 Recommended |
| 8 | LS-1 で「未マッチ層が約{失業者数}人」: 失業者全員が HW 未マッチであるかの誤誘導 | LS-1 | 🟡 Important |
| 9 | Panel 1 採用難度: 分母が「Agoop 平日昼滞在人口」のため、観光地・繁華街でスコア低下→「穴場」誤判定リスク | recruitment_diag Panel 1 | 🟡 Important |
| 10 | IN-1 発火条件 `!(0.05..=0.3).contains(&mw_share)` で発火: 通常医療福祉比率 (10-15%) は範囲内のためほぼ発火しない可能性 | IN-1 | 🟡 要検証 |

---

## 1. Insight 38 patterns 妥当性

### 1-1. HS (採用構造分析) 6 patterns

#### HS-1 慢性的人材不足 (`engine.rs:73-144`)
- **閾値**: VACANCY_WARNING=0.20, VACANCY_CRITICAL=0.30, VACANCY_TREND_THRESHOLD=0.25 (`helpers.rs:130-132`)
- **🔴 重大問題 (M-1)**: `v2_vacancy_rate` の定義は CLAUDE.md L223 で「recruitment_reason_code=1(欠員補充)の比率」と明記。これは「欠員補充を理由とする求人の割合」であり、厚労省「労働経済動向調査」の欠員率 (= (未充足求人 - 採用内定者) / (常用労働者数 + 未充足求人)) とは別物。
- **誤誘導**: body 文 `engine.rs:127` 「正社員の欠員率は{:.1}%です」は労働経済の欠員率と読まれる。実態は「欠員補充求人比率」。20-30% 閾値の業界根拠なし。
- **severity 判定**: 0.30 以上 + chronic で Critical の二段判定 (`engine.rs:99-107`) は構造的に妥当。
- **断定**: body 末尾「維持しています」 (`engine.rs:131`) はヘッジなし。phrase_validator 未適用。

#### HS-2 給与競争力不足 (`engine.rs:147-206`)
- **閾値**: SALARY_COMP_WARNING=0.90, CRITICAL=0.80 (`helpers.rs:135-136`)
- **計算**: competitiveness_index = local_mean / national_mean
- **妥当性**: 全国平均の 90%/80% 閾値は概念的に合理的だが、職種・産業を統制した中央値比較ではないため、産業構造が偏っている地域で誤発火しやすい。
- **断定**: 「不足しています」 (`engine.rs:181`) — phrase_validator 未適用。

#### HS-3 情報開示不足 (`engine.rs:209-271`)
- **閾値**: TRANSPARENCY_WARNING=0.50, CRITICAL=0.40 (`helpers.rs:139-140`)
- **業界根拠**: HRogレポート (2024) で求人開示率 50% 前後は中央値水準。閾値妥当。
- **問題**: 「応募率が低下する傾向があります」 (`engine.rs:251`) は HW 内応募率データを直接参照しないため推論経路が不透明。

#### HS-4 テキスト温度と採用難の乖離 (`engine.rs:274-321`)
- **🟡 重大問題**: TEMP_LOW_THRESHOLD = 0.0 (`helpers.rs:143`) — 温度スコアの物理単位や中央値が文書化されていない。発火条件 `vacancy_rate >= VACANCY_CRITICAL && temperature < 0.0` (`engine.rs:289`) で、温度スコアの実分布が判明しないと閾値妥当性を評価できない。
- **推奨**: temperature の元値分布 (P25/P50/P75) を確認し、絶対閾値ではなく相対閾値 (e.g., 県内 P25 未満) に変更。

#### HS-5 雇用者集中 (`engine.rs:324-369`)
- **閾値**: HHI_CRITICAL=0.25, TOP1_SHARE_CRITICAL=0.30 (`helpers.rs:146-147`)
- **業界根拠**: 米国 DOJ/FTC ガイドライン HHI > 0.25 = 高集中度。日本でモノプソニー議論で HHI > 0.20 (玄田 2020) もある。閾値妥当。
- **断定**: 「賃金水準をコントロールしている可能性があります」 (`engine.rs:349`) — 「可能性」ヘッジあり、ただし phrase_validator 未適用。

#### HS-6 空間的ミスマッチ (`engine.rs:372-422`)
- **閾値**: ISOLATION_WARNING=0.50, DAYTIME_POP_RATIO_LOW=0.90 (`helpers.rs:150-151`)
- **問題**: isolation_score の定義 (距離圏内 accessible 求人 / 自市区町村求人) は body から読み取れない。0.50 の意味不明。
- **断定**: 「求人エリアの拡大が有効です」 (`engine.rs:390`) — 断定。phrase_validator 未適用。

### 1-2. FC (将来予測) 4 patterns

#### FC-1 求人量トレンド (`engine.rs:444-486`)
- **🟡 問題**: `ctx.ts_counts` のサンプル件数バイアスへの注記がコード・body ともになし。Panel 6 (`market_trend.rs:78-90`) では is_sample 分岐があるが engine.rs 側にはなし。一貫性欠如。
- **forecast 計算**: `forecast_6m = latest * (1 + slope * 6)` (`engine.rs:462`) — 線形外挿で頭打ち・季節要因なし。CI も提示しないため過信誘導懸念。
- 短期 (6 ヶ月) に限れば許容範囲。

#### FC-2 給与上昇圧力 (`engine.rs:489-540`)
- **計算**: salary_slope (月次) vs wage_slope_monthly (年次/12) を直接比較 (`engine.rs:507`)。
- **🟢 軽微**: 最低賃金は通常年1回 (10月) の階段関数的更新のため、線形回帰の slope は中央値的解釈で妥当。データ年数不足時に過小評価可能。
- **断定**: 「ほぼ同水準です」 (`engine.rs:514`) — phrase_validator 未適用。

#### FC-3 人口動態 (`engine.rs:543-637`)
- **計算**: 55歳以上 / 生産年齢 = 退職予備率 (`engine.rs:575`)
- **閾値**: 0.30 + net_migration<0 で Critical, 0.25 で Warning (`engine.rs:588-594`)。
- **業界根拠**: 内閣府高齢社会白書では「退職世代率」標準閾値はないが、20-30% は経験的に有意な水準。
- **断定**: 「10年以内に大量退職が見込まれます」 (`engine.rs:610`) は「ヘッジ」なし。phrase_validator 未適用。

#### FC-4 充足困難度悪化 (`engine.rs:640-700`)
- **計算**: avg_listing_days と churn_rate の slope 同時悪化判定。
- **問題**: severity 閾値 (`days_slope > 0.03 && churn_slope > 0.02`) (`engine.rs:665`) の 0.03/0.02 (= 月次 3% / 2%) の根拠不明。HW 月次データの典型変動幅と整合確認要。

### 1-3. RC (地域比較) 3 patterns

#### RC-1 ベンチマーク順位 (`engine.rs:719-763`)
- **閾値**: composite < 30 で Warning, > 70 で Positive (`engine.rs:730-736`)
- **問題**: composite_benchmark の 0-100 スケール計算式が region_benchmark テーブル仕様未確認のため評価不能。

#### RC-2 給与・休日地域差 (`engine.rs:766-829`)
- **🟢 問題**: 閾値 -20000円/+10000円 (`engine.rs:796-802`) は固定値で、職種別給与水準 (例: 製造業 vs IT) を考慮しない。介護職と IT エンジニアで同じ閾値は不適切。
- **推奨**: 標準偏差や percentile での相対閾値化。

#### RC-3 人口×求人密度 (`engine.rs:832-898`)
- **✅ 評価**: GE-1 との矛盾誤解防止のため caveat (`engine.rs:864-867`) が明記されている。Cross-reference 設計が適切。
- **閾値**: density > 50 件/千人で Warning, < 5 件/千人で Positive
- **問題**: `vacancy.total_count` を求人総数として SUM (`engine.rs:844`) するが、これは雇用形態別行を全合算しているため総求人数として妥当。ただし「求人密度=1000人あたり件数」の業界ベンチマーク (例: 全国平均) が body にないため、50/5 の解釈が直感的でない。

### 1-4. AP (アクション提案) 3 patterns

#### AP-1 給与改善 (`engine.rs:928-971`)
- **計算**: 全国中央値 - 自地域平均 で必要増額。年間人件費増 = 増額 × 12 (`engine.rs:943`)。
- **🟢 軽微**: 賞与・社会保険料・退職金を考慮しないため、実コストを過小推定。実務的には ×16 (賞与4ヶ月想定) や法定福利費約16% 追加が必要。
- **断定**: 「全国中央値に到達できます」 (`engine.rs:951`) — できる断定。phrase_validator 未適用。

#### AP-2 求人原稿改善 (`engine.rs:974-1017`)
- **閾値**: 開示率 < 0.30 で「未開示」判定 (`engine.rs:991`)。
- **問題**: 0.30 の根拠不明。HW では特定項目 (女性比率等) の開示が法的・実務的に困難な場合があり、すべてを「開示せよ」と指示するのは不適切。

#### AP-3 採用エリア拡大 (`engine.rs:1020-1047`)
- **閾値**: daytime_ratio < 1.0 で発火 (`engine.rs:1027`)。
- 「可能性」ヘッジあり。妥当。

### 1-5. CZ/CF (通勤圏) 6 patterns

#### CZ-1 通勤圏人口ポテンシャル (`engine.rs:1084-1128`)
- 閾値: local_share < 0.05 で Positive。妥当。

#### CZ-2 通勤圏給与格差 (`engine.rs:1131-1180`)
- 閾値: ±5%/-10% (`engine.rs:1151-1156`)。妥当な経験閾値。

#### CZ-3 通勤圏高齢化 (`engine.rs:1183-1219`)
- 閾値: 0.20/0.30 → Info/Warning (`engine.rs:1188,1195`)
- 業界標準: 高齢化率 21% 以上で「超高齢社会」(WHO/総務省定義)。閾値妥当。

#### CF-1 実通勤フロー (`engine.rs:1224-1277`)
- 閾値: actual_ratio < 0.01 で Warning (`engine.rs:1239`)。やや極端。

#### CF-2 流入元ターゲティング (`engine.rs:1280-1306`)
- 業界標準: 通勤OD 国勢調査ベース。妥当。
- 断定: 「応募者プールの拡大が見込めます」 (`engine.rs:1294`) — phrase_validator 未適用。

#### CF-3 地元就業率 (`engine.rs:1309-1355`)
- 閾値: 0.7/0.3 (`engine.rs:1314-1318`)。妥当。

### 1-6. StructuralContext 6 patterns (LS/HH/MF/IN/GE)

これら 6 パターンは `assert_valid_phrase` 呼出済 (`engine.rs:1368,1372,1376,1380,1384,1388`)。文末ヘッジ強制機構が動作。

#### LS-1 採用余力 (`engine.rs:1399-1445`)
- **閾値**: UNEMPLOYMENT_RATE_MULTIPLIER_WARNING=1.2, CRITICAL=1.5 (`helpers.rs:161-162`)
- **🟡 重大問題**: body 「未マッチ層が約{失業者数}人いる可能性があります」 (`engine.rs:1426`) は失業者全員が HW 求人と未マッチであるかのような誤誘導。失業者には自営業希望者・他媒体応募者・非労働力化準備中者が含まれる。「未マッチ層」用語の使用自体が因果断定的。
- **推奨**: body を「失業者数約{}人 (HW 以外への応募状況は本データから判定不可)」に変更。

#### LS-2 産業偏在 (`engine.rs:1451-1501`)
- **閾値**: TERTIARY_CONCENTRATION_THRESHOLD=85.0%, PRIMARY=20.0% (`helpers.rs:165-166`)
- **業界根拠**: 全国第3次産業比率 約74% (2020年国勢調査)、第1次 約3.4%。85% / 20% は明らかな偏在水準。閾値妥当。

#### HH-1 単独世帯 (`engine.rs:1506-1543`)
- **閾値**: SINGLE_HOUSEHOLD_RATE_THRESHOLD=40% (`helpers.rs:169`)
- **業界根拠**: 全国単独世帯率 38% (2020年国勢調査)。40% 以上で「比較的高い」は妥当。
- 県平均比較 (`engine.rs:1521-1524`) も実装。✅

#### MF-1 医療供給密度 (`engine.rs:1550-1609`)
- **🔴 重大バグ疑い (M-6)**:
  - コメント `engine.rs:1565` 「2022年公式: 約27人/10万人 = 2.7人/1万人」
  - 定数 `NATIONAL_PHYSICIANS_PER_10K: f64 = 27.0` (`engine.rs:1565`)
  - 計算: `local_density = physicians / total_pop * 10_000.0` → 「人/1万人」を出力
  - 比較: `ratio = local_density / 27.0` → 「人/1万人」を「27 (実は人/10万人)」で割る
  - **結果**: ratio が 1/10 になり、すべての市区町村で「全国の 10% 未満」と誤判定 → 全市区町村で MF-1 発火
- **要検証**: physicians テーブルが「人」単位か単位確認、コメント or 定数のどちらが正しいか確定。

#### IN-1 産業構造ミスマッチ (`engine.rs:1616-1660`)
- 計算: 医療福祉事業所比率のみ。コサイン類似度実装ではない簡易版。
- **🟡 発火条件反転疑い (M-7)**: `!(0.05..=0.3).contains(&mw_share)` で発火 (`engine.rs:1637`) — これは「mw_share が 5%-30% の範囲外」だが、コメント (`engine.rs:1611-1614`) では「医療福祉比率と HW 全体欠員率の乖離」を意図。range の使い方が逆 (典型値は 10-15%) の可能性。
- 5% 未満 (極端な医療福祉不足) または 30% 超 (極端な集中) で発火する仕様なら理解できるが、body「事業所のうち医療・福祉が{:.1}%を占めており、HW求人職種分布と構造的な乖離がある可能性」 (`engine.rs:1648-1651`) はこの両極端で同じ文言になり違和感あり。

#### GE-1 可住地密度 (`engine.rs:1666-1740`)
- **✅ 評価**: RC-3 との cross-reference (`engine.rs:1717-1718`) が明記。
- **閾値**: HABITABLE_DENSITY_MAX=10000, MIN=50, CRITICAL_MAX=20000, CRITICAL_MIN=20 (`helpers.rs:180-183`)
- **業界根拠**: 全国平均可住地人口密度 約880人/km²。10000 (東京区部レベル) / 50 (山間部レベル) は妥当。

### 1-7. SW-F01〜F10 (Agoop 人流) 10 patterns

#### SW-F01 夜勤需要 (`engine_flow.rs:43-70`)
- 閾値: midnight_ratio >= 1.2 (Warning), >= 1.5 (Critical)
- **🟢 軽微**: 深夜時間帯滞在が昼の 1.2 倍以上は商業地・歓楽街・病院密集地に偏在。介護・看護・警備の夜勤需要との直接的関連は単なる相関で、body「採用機会を検出できる傾向」 (`engine_flow.rs:59`) は踏み込みすぎ。phrase_validator は通過しているが意味的に楽観的。

#### SW-F02 休日商圏 (`engine_flow.rs:73-95`)
- 閾値: holiday_day_ratio >= 1.3 → 妥当。

#### SW-F03 ベッドタウン (`engine_flow.rs:98-125`)
- 閾値: daynight_ratio < 0.8 かつ outflow >= 0.2 → 妥当。

#### SW-F04 メッシュギャップ (`engine_flow.rs:128-141`)
- **🟡 未実装**: 関数本体で `None` 返却 (`engine_flow.rs:140`)。プレースホルダ。発火しない。

#### SW-F05 観光ポテンシャル (`engine_flow.rs:144-166`)
- 閾値: holiday_day_ratio >= 1.5
- **🟡 矛盾 (M-2)**: SW-F02 と同じ holiday_day_ratio を使い、1.3 で「休日商圏不足」、1.5 で「観光ポテンシャル」の両発火が起こる。1.5 以上で両方鳴り、方向性の異なる示唆が併発する。
- **推奨**: SW-F05 発火時は SW-F02 を抑制する logic か、F02 上限 1.5 未満を追加。

#### SW-F06 コロナ回復 (`engine_flow.rs:169-192`)
- **🟡 仕様不一致**: helpers.rs:204-205 では「2021人流/2019 > 0.9 AND 2021求人/2019 < 0.8」だが実装は人流側のみ判定 (`engine_flow.rs:171`)。求人側未参照のため、body「採用マインドの慎重化の可能性を評価できます」 (`engine_flow.rs:181-183`) と記述するのは現実装では実態と乖離。

#### SW-F07 広域流入 (`engine_flow.rs:195-217`)
- 閾値: 15% — Round 1-3 で Agoop ベース実装済。妥当。

#### SW-F08 昼間労働力プール (`engine_flow.rs:220-243`)
- 閾値: daynight_ratio >= 1.3 → 妥当。
- **🟢 矛盾 (M-3)**: SW-F03 (ベッドタウン daynight<0.8) と SW-F08 (昼間プール daynight>=1.3) は排他的だが、daynight が 0.8-1.3 のレンジでは両方発火しない (中間地域の沈黙)。改善余地。

#### SW-F09 季節雇用 (`engine_flow.rs:246-269`)
- 閾値: monthly_amplitude >= 0.3 → 妥当。

#### SW-F10 企業立地マッチ (`engine_flow.rs:272-278`)
- **🟡 未実装**: `None` 返却 (`engine_flow.rs:277`)。プレースホルダ。発火しない。

---

## 2. 給与統計の妥当性

### 2-1. ネイティブ単位集計 (`aggregator.rs:553-672`)

#### 構造
- 正社員グループ → 月給ベース (`aggregator.rs:614-615`)
- パートグループ → 時給ベース (`aggregator.rs:616`)
- 派遣・その他 → 多数派の salary_type で動的決定 (`aggregator.rs:617-624`)

#### 換算式 (`aggregator.rs:582-606`)
- Hourly → Monthly: `v * 160` (1日8h × 20日)
- Monthly → Hourly: `v / 160`
- Annual → Monthly: `v / 12`
- Daily → Monthly: `v * 20`、Daily → Hourly: `v / 8`
- Weekly → Monthly: `v * 4`、Weekly → Hourly: `v / 40`

**🟡 問題**:
1. 月160h は厚労省「就業条件総合調査」所定労働時間 (165-170h) より過小。160 vs 170 で 6% 誤差。
2. Daily → Monthly = ×20 は「20日労働 × 1日8h」前提だが、Weekly → Monthly = ×4 は週単位なら ×4.33 が正確 (52週/12月 = 4.33)。

### 2-2. IQR 外れ値除外 (`statistics.rs:163-228`)

- 1.5 × IQR は Tukey の標準箱ひげ図定義。**業界標準と整合**。✅
- n<4 で全件通過 (`statistics.rs:177`) — IQR 計算不能のため安全側。妥当。
- IQR=0 の場合全件通過 (`statistics.rs:184-186`) — ゼロ分散保護。妥当。

### 2-3. Bootstrap 95% CI (`statistics.rs:21-69`)

- 反復回数: 2000 (`statistics.rs:285`)。標準的 (1000-10000)。妥当。
- パーセンタイル法で `iterations * 0.025`/`*0.975` (`statistics.rs:56-57`)。
- **🟢 軽微**: BCa (Bias-corrected accelerated) ではなく単純パーセンタイル法のため、歪みが大きい分布で CI が偏る。給与は典型的に右に歪むため、過小推定の懸念。

### 2-4. Trimmed mean (`statistics.rs:83-114`)

- 上下 10% トリム (`statistics.rs:289`)。標準的 (5-20%)。妥当。
- n <= 2*trim_count で通常平均を返す (`statistics.rs:94-102`)。妥当。

### 2-5. 計算式の細部

- mean: `sum / n` — i64 除算で小数切り捨て (`statistics.rs:266`)。1円単位なので影響軽微。
- median: 偶数時は隣接2値平均、奇数時は中央値 (`statistics.rs:269-273`)。標準。
- std_dev: 母分散 (n 除算) (`statistics.rs:277-282`) — 標本分散 (n-1 除算) ではない。サンプル統計量としては偏りあり。

### 2-6. Panel 5 条件ギャップ給与計算 (`condition_gap.rs:115-126`)

- annual_income = salary_min × (12 + bonus_months)
- **🟡 問題**: 「salary_min」 (募集給与の下限) を年収換算ベースに使用。実態の平均年収より系統的に低くなる。median ベースだが、HW 求人の salary_min は「最低保証」傾向があり、平均年収との乖離が大きい。
- **✅ 評価**: 注記 (`condition_gap.rs:8-12`) で「HW 慣習として市場実勢より給与を低めに出すケースあり」明記。

---

## 3. 雇用形態の分類

### 3-1. survey 側 (`aggregator.rs:675-687`)

```
パート/アルバイト → "パート"
正社員/正職員/契約/業務委託 → "正社員"
それ以外 → "派遣・その他"
```

### 3-2. recruitment_diag 側 (`mod.rs:74-81`)

```
"正社員" → ["正社員"]
"パート" → ["パート労働者", "有期雇用派遣パート", "無期雇用派遣パート"]
"その他" → ["正社員以外", "派遣", "契約社員"]
```

### 3-3. market_trend 側 (`market_trend.rs:178-185`)

```
"正社員"/"パート"/"その他" → 同名 (V2 emp_group は3値)
"アルバイト" → "パート"
```

### 3-4. 🟡 重大不整合

| 雇用形態 | survey aggregator | recruitment_diag expand | 結果 |
|----------|-------------------|--------------------------|------|
| 契約社員 | "正社員" グループ | "その他" 展開先 | **不整合** |
| 業務委託 | "正社員" グループ | どこにも分類されず空フィルタ | **不整合** |
| 派遣 (フル) | "派遣・その他" グループ | "その他" 展開先 | 一致 |
| 派遣パート | "パート" グループ | "パート" 展開先 | 一致 |

**問題点**:
1. **業務委託の月給扱い**: `aggregator.rs:678-682` で業務委託 → 正社員グループ → 月給ベース集計。実態は業務委託は「報酬」概念で、月給制ではない。これを正社員月給と並べて集計すると、業務委託契約金 (例: 月50万円固定) が正社員給与の中央値を歪める可能性。**雇用形態混在による誤誘導の典型例**。
2. **契約社員の二重定義**: 同じ UI から見える数値が、media タブと診断タブで別グループの集計値になる。

**推奨**: 統一定義モジュール (`emp_classifier.rs`) を作成し、両所から呼び出す。

---

## 4. 地域分析の妥当性

### 4-1. 同名市区町村

`city_name_to_code` (`city_code.rs:41-45`) は (prefcode, city_name) のタプルキーで HashMap 引き。
- 「伊達市」(北海道 01236 vs 福島県 07213) → 異なる citycode で正しく区別。✅
- 「府中市」(東京都 13206 vs 広島県 34208) → 同上。✅

InsightContext 等でも `(pref, muni)` 文字列タプルで処理 (`engine.rs:744`)。SQLite クエリも prefecture+municipality WHERE 句 (`fetch.rs:64`)。✅ 同名市区町村バグなし。

### 4-2. 政令指定都市の区別

- master_city.csv の citycode 構造: 札幌市中央区=1101, 大阪市北区=27127 (`city_code.rs:54-58`) — 区単位。
- HW DB の `postings.municipality` 値が「横浜市西区」か「横浜市」かは ETL 仕様 (`hellowork_etl.py`) を本監査では未確認。
- **要検証**: posting データの municipality カラムが区単位/市単位どちらか。混在時は集計バグの温床。

### 4-3. 通勤圏分析 (Agoop / 国勢調査OD)

- CZ-1〜3: 距離ベース 30km 圏内
- CF-1〜3: 国勢調査 2020年 OD ベース
- CZ と CF の重複発火: 同一地理事象に対して距離ベース・実フローベースの 2 つの示唆が併存。両方順番に呼出 (`engine.rs:1056-1080`)。冗長性あり。

---

## 5. 採用診断 8 panels の指標

### 5-1. Panel 1 採用難度スコア (`handlers.rs:91-311`)

- 計算式: `score = HW該当求人件数 / Agoop平日昼滞在人口 × 10000` (`handlers.rs:194-198`)
- **🟡 重大問題1 (#9)**: 分母が「平日昼滞在人口」なので、観光地・繁華街・商業集積地では昼間滞在が膨張 → スコア低下 → 「穴場」誤判定。
  - 例: 銀座・京都四条河原町は昼間滞在が多 → 求人が多くてもスコア下がる。
- **🟡 問題2**: rank 閾値 (1.0/3.0/7.0/15.0 件/万人) (`handlers.rs:261-310`) の根拠不明。「経験則的な閾値」と注記 (`handlers.rs:260`) はあるが、職種別差異を考慮しない。
- **✅ 評価**: hw_count==0, population<=0 の早期リターン (`handlers.rs:243-257`)、so_what テキストで「傾向」「可能性」表現使用。

### 5-2. Panel 2 人材プール (`handlers.rs:313-440`)

- 計算: day_population - night_population = commuter_inflow (`handlers.rs:383`)
- 妥当: 国勢調査の昼夜間人口比と整合する概念。
- **🟢 軽微**: Agoop 滞在人口は観光客等を含むため、純粋な「労働力プール」ではない。

### 5-3. Panel 3 流入元分析

- Agoop fromto_city 系テーブル使用と推定。詳細 SQL 未読了。

### 5-4. Panel 5 条件ギャップ (`condition_gap.rs`)

- 中央値計算: ORDER BY LIMIT 1 OFFSET N/2 (`condition_gap.rs:282-285`)。SQL median 関数なしの正攻法。
- **🟡 重大問題**: emp_type フィルタが UI 値そのまま (`condition_gap.rs:177-180`)。`expand_employment_type` を使わないため、UI「パート」を選ぶと postings.employment_type='パート' で検索するが、実値は「パート労働者」「有期雇用派遣パート」等。**ヒット件数 0 で「データ不足」誤表示** の可能性大。
- **✅ 評価**: salary_type='月給' フィルタ (`condition_gap.rs:167`) で時給データ混入を防止。雇用形態混在防止策。

### 5-5. Panel 6 市場動向 (`market_trend.rs`)

- **✅ 評価**: is_sample 分岐 (`market_trend.rs:78`) で job_type 指定時の ts_turso_salary 由来サンプル件数を「業界サンプル件数」と明示 (`market_trend.rs:81-85`)。誤誘導防止策として適切。
- 増加率: (last - first) / first × 100 (`market_trend.rs:265-275`)。月数2-24クランプ。妥当。

### 5-6. Panel 7 穴場マップ (`opportunity_map.rs`)

- 詳細未読了。Panel 1 の市区町村展開版と推定。

### 5-7. Panel 8 AI示唆統合 (`insights.rs`)

- 38 patterns の統合配信 (`insights.rs:369`)。

---

## 6. 媒体分析 (survey) の妥当性

### 6-1. 雇用形態別 IQR の挙動

雇用形態グループ内で IQR 1.5 適用 (`aggregator.rs:634-637`)。グループ間の異質性を吸収するため適切。
- **問題**: グループ内サンプル <= 4 だと IQR 計算不能で全件通過。少件数地域で外れ値が残存。

### 6-2. HW 連携セクション (`hw_enrichment.rs`)

- `enrich_areas` (`hw_enrichment.rs:77-`)
- **🔴 重大問題 (#4)**: posting_change_3m_pct / _1y_pct を HwAreaEnrichment (キー = "{prefecture}:{municipality}") に格納するが、実装 (`hw_enrichment.rs:108-128`) では prefecture 単位で fetch して各 muni に流し込む。**同じ都道府県の全市区町村が同一の change_pct を持つ**。
- **誤誘導**: 「○○市の 3ヶ月人員推移 +20%」と表示されると、その市の独自データに見えるが、実態は都道府県全体の値。
- **推奨**: ts_turso_counts に municipality 粒度を追加するか、UI で「※都道府県全体の値」と注記。

### 6-3. 地域注目企業 (SalesNow) 人員推移

- 1y 閾値: ±10% / ±30% (`hw_enrichment.rs:56-65`)
- 3m 閾値: ±3% / ±15% (`hw_enrichment.rs:45-54`)
- **業界根拠**: SalesNow データは法人登記・採用情報等の集約で、月次精度では揺らぎが大きい。±3% を「緩やかに増加/減少」とするのはやや感度高い。
- **✅ 評価**: 注記 (`integration.rs:432`) で「直近の組織改編や統計粒度による揺らぎを含みます」と明記。

### 6-4. 散布図 R² 解釈

- linear_regression_points (`aggregator.rs:690-732`) で R² 計算。ss_tot=0 なら 0.0 (`aggregator.rs:721-725`)。妥当。
- **🟡 問題**: R² 値の UI 表示時に「強い相関」「弱い相関」等のラベル化基準は本監査未確認。R² が散布図で誤解されやすいので注記必須。

---

## 矛盾・誤誘導の発見

### M-1: vacancy_rate の概念混乱 (🔴 Critical)

CLAUDE.md L223 で `v2_vacancy_rate` = 「recruitment_reason_code=1(欠員補充) の比率」と定義されているが、HS-1, HS-4, FC-4, RC-3 など複数 insight で「欠員率」として表示。労働経済統計の欠員率 (=未充足求人/総常用労働者数) ではない。**全社的な定義整理が必要**。

### M-2: SW-F02 vs SW-F05 の同時発火矛盾 (🟡)

holiday_day_ratio が 1.5 以上で両方発火。前者は「人材不足」後者は「観光ポテンシャル」と異なる方向性。

### M-3: SW-F03 vs SW-F08 の中間地域沈黙 (🟢)

daynight_ratio が 0.8-1.3 の市区町村は両方発火しない。中間地域構造が示唆対象外。

### M-4: 雇用形態分類の二重定義 (🟡)

§3-4 参照。survey と recruitment_diag で契約社員・業務委託の所属グループが異なる。

### M-5: HS-1 等の vacancy_rate × 100 表記の整合性 (🟡)

`engine.rs:128` で `vacancy_rate * 100.0` を表示。vacancy_rate が DB 上で 0-1 / 0-100 のいずれで格納されるかが ETL 仕様未確認のため不確定。整合性要検証。

### M-6: MF-1 単位混乱 (🔴 要検証)

§1-6 参照。`NATIONAL_PHYSICIANS_PER_10K = 27.0` とコメント「2.7人/1万人」が 10倍食い違い。`physicians / total_pop * 10000` で人/1万人を出して 27 と比較すると、ratio が 1/10 になり、すべての市区町村で「医師不足」発火する可能性。

### M-7: IN-1 発火条件反転疑い (🟡)

§1-6 参照。`!(0.05..=0.3).contains(&mw_share)` で発火 (`engine.rs:1637`)。これは「mw_share が 5%-30% の範囲外」だが、コメント (`engine.rs:1611-1614`) では「医療福祉比率と HW 全体欠員率の乖離」を意図。range の使い方が逆 (典型的な値は 10-15%) の可能性。

### M-8: SW-F06 仕様と実装の乖離 (🟡)

§1-7 SW-F06 参照。仕様では「人流回復 AND 求人遅延」AND 条件、実装は人流のみ。

---

## 優先 Top 10 改善項目

| 順位 | ID | 内容 | 影響 | 工数 |
|------|----|------|------|------|
| 1 | M-6 | MF-1 単位バグ (定数 vs コメント 10倍ズレ) の検証・修正 | 🔴 全市区町村誤発火 | 小 |
| 2 | M-1 | vacancy_rate の定義統一 (DB ETL 段階で命名変更 or UI 表記注意) | 🔴 全 vacancy 言及 insight | 中 |
| 3 | §6-2 | HW 連携 3m/1y 変化率を都道府県粒度と明示 | 🟡 媒体分析誤誘導 | 小 |
| 4 | §5-4 | Panel 5 emp_type フィルタを expand_employment_type 経由に修正 | 🟡 Panel 5 ヒット 0 防止 | 小 |
| 5 | M-7 | IN-1 発火条件の確認・修正 | 🟡 IN-1 ロジック誤り疑 | 小 |
| 6 | §3-4 | 雇用形態分類の統一モジュール化 | 🟡 survey/diag 整合 | 中 |
| 7 | §1-6 LS-1 | 「未マッチ層」用語廃止 | 🟡 LS-1 誤誘導 | 小 |
| 8 | §1-1 全般 | engine.rs 22 patterns に assert_valid_phrase 適用 | 🟡 断定表現混在 | 中 |
| 9 | §1-1 HS-4 | TEMP_LOW_THRESHOLD=0.0 の根拠調査・相対閾値化 | 🟡 HS-4 | 中 |
| 10 | §5-1 | Panel 1 採用難度の観光地補正 (例: 居住人口併用) | 🟡 観光地誤判定 | 中 |

---

## 残課題

### 未確認領域
- postings テーブルの municipality 値が区単位か市単位か (政令指定都市)
- region_benchmark の composite_benchmark 計算式
- opportunity_map (Panel 7) の詳細ロジック
- SalesNow growth ratio の元データ精度
- temperature/urgency_density の元値分布
- v2_vacancy_rate の実 DB 値スケール (0-1 vs 0-100)

### 要追加検証
- HS-4 temperature の P25/P50/P75 分布
- vacancy_rate 値範囲の実データ確認
- MF-1 physicians テーブルの単位 (人 / 10k人 / 100k人)
- IN-1 establishments.industry='850' の mw_share 分布

### 設計レベル課題
- 38 パターンの優先順位付け不在 (Severity ソートのみ)
- 同時発火パターンの相互参照テーブルが GE-1↔RC-3 のみ。AP-1 は HS-2 依存 (`engine.rs:906-910`) だが他の依存ペアの cross-ref が未整備
- 業界・職種別閾値の動的化未対応 (現状は全職種同一閾値)

### テスト視点 (本監査外だが推奨)
- 既存 22 パターン body の phrase_validator 通過テスト追加
- Panel 1 の観光地サンプル (例: citycode 13104=新宿区) でのスコア妥当性逆証明
- vacancy_rate 0-100 / 0-1 混在の境界値テスト
- SW-F02 vs SW-F05 同時発火検出テスト

---

## 監査根拠ファイル

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\engine.rs` (1740行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\engine_flow.rs` (359行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\helpers.rs` (220行: 閾値定数)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\phrase_validator.rs` (123行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\aggregator.rs` (675-687行: classify_emp_group_label, 553-672行: aggregate_by_emp_group_native)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\statistics.rs` (419行: IQR/Bootstrap/Trim)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\hw_enrichment.rs` (HW連携)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\integration.rs` (地域注目企業)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\handlers.rs` (Panel 1-3)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\mod.rs` (74-81: expand_employment_type)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\condition_gap.rs` (Panel 5)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\market_trend.rs` (Panel 6)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\geo\city_code.rs` (citycode 解決)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\CLAUDE.md` L223 (v2_vacancy_rate 定義)
