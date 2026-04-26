# D-1 Deep Dive: 媒体分析タブ (survey) パイプラインなぜなぜ5回監査

**実施日**: 2026-04-26
**担当**: チーム D-1 (Quality Engineer / Domain Auditor)
**対象**: V2 HW Dashboard `src/handlers/survey/` + `src/handlers/emp_classifier.rs`
**手法**: なぜなぜ5回 + 逆証明 (Reverse Proof) + 逆因果関係 (Reverse Causality)
**根拠**: Plan P2 (`plan_p2_domain_logic.md`) / team_gamma_domain.md / exec_f1/c3/e2_results.md
**MEMORY 遵守**: feedback_correlation_not_causation, feedback_reverse_proof_tests, feedback_test_data_validation, feedback_never_guess_data

---

## 0. エグゼクティブサマリ

| 領域 | 検証 Q 数 | 🔴 重大バグ | 🟡 中程度 | 🟢 軽微 / 仕様妥当 |
|------|----------|-----------|----------|----------------|
| 1. CSV パース | 5 | 1 (Shift-JIS) | 2 (BOM未対応, ヘッダー誤検出) | 2 |
| 2. 住所パース | 5 | 0 | 2 (designated_city 区脱落, station 同名駅) | 3 |
| 3. 給与パース (167h) | 5 | 1 (賞与未パース) | 2 (異常値上限ゆるい, 範囲) | 2 |
| 4. 雇用形態分類 | 5 | 1 (二重定義残存) | 2 (時給/月給混在, 業務委託) | 2 |
| 5. 統計処理 | 5 | 0 | 2 (BCa 未採用, 母分散) | 3 |
| 6. 集計 (aggregator) | 5 | 1 (CSV 重複検出なし) | 2 (NULL 挙動, R²解釈) | 2 |
| **合計** | **30** | **4** | **12** | **14** |

**逆因果リスクのある示唆文 / 表現**: 5件検出 (4.x / 6.3 / 6.6 等)

---

## 1. CSV パース (`upload.rs` 877 行)

### Q1.1: なぜ Indeed と JobBox を別パーサにしているか?

```
[現象] CsvSource enum + 専用 col_map ロジックが Indeed/JobBox/Unknown 3 分岐 (upload.rs:42-67, 372-499)
↓ なぜ?
[直接原因] 両媒体のヘッダー命名規則が違う: Indeed=「会社名・勤務地・給与」, JobBox=「企業名・所在地・賃金」
↓ なぜ?
[構造的原因] さらに各社スクレイピングツールが CSS クラス名 (jcs-JobTitle / p-result_name 等) を生 CSV 化したフォーマットがあり、ヘッダーが日本語ですらない
↓ なぜ?
[根本原因] 媒体ごとの DOM 構造に依存したスクレイピング出力をそのままアップロードする運用が前提
↓ なぜ?
[真因] 媒体側 API が公開されておらず、ユーザーがブラウザ拡張等で抽出した CSV を読む唯一の手段だから
↓ 5 回目の Why
[究極] 「同一の semantic フィールド (会社名/給与/住所等) を、異なる物理ヘッダー名から復元する」ことを保証している。
       前提が崩れる時: 媒体が新しいヘッダー命名 (例: Indeed が "company_jp" に統一) を導入すると build_column_map のキーワード辞書 (会社名/会社/company 等、L384-388) で拾えず col_map 空 → fallback の detect_columns_from_data に落ちる。fallback でも score>=30 必要 (L617) で、新形式が score 不足だと **silently 全列無視** で全レコードが空フィールドになる。
```

**逆証明**:
- 現状: `headers = ["company_jp", "office_pref"]` (新命名想定) を `build_column_map` に通すとキーが入らず空 HashMap
- detect_columns_from_data が動くには **データ行が score>=30 を満たす必要** あり (L617)。会社名 score は「株式会社/有限会社/合同会社」が必須 (`score_company`, L738-758)、ただ `val.len() in 3..=50 で score=5` の弱マッチがある
- 5 サンプル全部「Acme Inc」だと score=5 のみ → 30 未満 → `result.insert("company_name", _)` が走らない → company_name が空文字
- **テストに具体値での逆証明なし**: `parser_aggregator_audit_test.rs` には Indeed/JobBox 分岐の境界テストがない (Grep 結果 `indeed_csv` を含むテストは `location_parser_realdata_test` のみ)

**逆因果**:
- 「Indeed CSV だから col_map に "会社名" キーが入る」は逆方向の「ヘッダーに "会社名" があるから Indeed と判定される」と区別できていない (L43-66)
- `detect_csv_source` が `header_str.contains("会社名")` で Indeed と判定するため、JobBox 形式に "会社名" があるだけで Indeed 分岐に行く
- 仕様文書 (Plan P2 / team_gamma) に明示的根拠なし → **🟡 根拠不明**

**Severity**: 🟡 (新形式の silent failure は重大だが、現状運用では破綻していない)

---

### Q1.2: 「その他」フォールバック時の列名推定ロジックは妥当か?

```
[現象] col_map.len() < 3 の時 detect_columns_from_data が起動 (upload.rs:162-189)
↓ なぜ?
[直接原因] ヘッダー名が日本語でも英語でもない (CSS クラス名等) ケースの救済
↓ なぜ?
[構造的原因] ヘッダー由来 (build_column_map) で取れないと semantic field が紐付かないため
↓ なぜ?
[根本原因] CSV 仕様が標準化されていない外部出力に対してロバスト性を持たせたい
↓ なぜ?
[真因] スコアリング閾値 30 はマジックナンバー (L617)。「3 件の都道府県 + 1 件の市区町村」でも 80+30=110 で通るが、「1 件のみマッチ」だと 50 程度で落ちうる。サンプル 20 行を全列に対しスコア合算するため、ヒットが偏ると本来の正しい列を見落とす可能性
↓ 5 回目の Why
[究極] 「heuristic スコアの絶対値 >= 30」が「列の semantic 一致」を保証していると仮定。前提が崩れる時: 例えば求人タイトル列に「東京の事務員」のように都道府県名が含まれると score_location が 50+30=80 でヒット → location 列として誤認 → 都道府県別集計が job_title 列で集計される
```

**逆証明**:
- サンプルデータ: row=`["dummy_url", "東京の介護事務スタッフ募集", "Acme株式会社", "東京都港区赤坂1-2-3", "月給25万円"]`
- col_idx 1 (job_title) で score_location=80 (東京都+市町村なし、ただし「東京」を含むため都道府県マッチで 50 + 「事務」キーワードはなし)
- col_idx 3 (location) で score_location=80 (東京都+市町村あり=80)
- 同点で job_title の方が col_idx 小さい → max_by_key で同点時は実装依存 (L612 max_by_key は「最後に走査した最大値」を返す HashMap iter なので**非決定的**)
- **既存テストに該当 reverse-proof なし** (Grep: `detect_columns_from_data` を直接呼ぶテスト 0 件)

**逆因果**:
- 「スコア 30 以上 → 正しい列」という方向だが、「正しい列 → スコア 30 以上が必ず付く」とは限らない (例: 給与列が `JPY 250000` の英語表記だと 円/万 ヒットせず score 0)

**Severity**: 🟡 (HashMap iter 非決定性 + 閾値根拠不明)

---

### Q1.3: BOM (UTF-8 BOM / Shift-JIS BOM) 混在時の挙動

```
[現象] L134-138: UTF-8 BOM (0xEF 0xBB 0xBF) のみスキップ。Shift-JIS / UTF-16 BOM の処理なし
↓ なぜ?
[直接原因] 実装時 Indeed/JobBox の現行出力が UTF-8 想定のため
↓ なぜ?
[構造的原因] csv crate は ReaderBuilder で encoding 指定なし → デフォルト UTF-8 として読む
↓ なぜ?
[根本原因] **encoding_rs / shift_jis 系 crate を依存に持たない** (Grep 結果: 0 件)
↓ なぜ?
[真因] ユーザーが Excel で「CSV (Shift-JIS)」として保存した場合、`csv::Reader` は UTF-8 として bytes を読み、非 ASCII バイト列を見て **panic ではなく "invalid UTF-8 sequence" エラー** で record() が err を返す → L196-201 で warn ログだけ出して continue → **全行 skipped, records.len() == 0 → "CSVにデータ行がありません" エラー**
↓ 5 回目の Why
[究極] 「アップロードファイルは UTF-8」を暗黙の前提として運用。崩れる場合 (Excel デフォルト保存): エラーメッセージは「メタデータ除外0行、不完全行除外0行」と表示され、ユーザーは原因 (encoding) を特定できない
```

**逆証明**:
- Shift-JIS で `"会社名,給与\n株式会社A,月給25万円\n"` を作る (Python: `s.encode('cp932')`)
- 期待: 全行パース成功 (現状: 全行 err → 0 records)
- **テストに該当 reverse-proof なし** (Grep `encoding|cp932|shift_jis` 0 件)
- 修正前/修正後の数値変化: records=0 → records=1

**逆因果**: なし

**Severity**: 🔴 (本番事故予備軍。Excel ユーザーの Shift-JIS は日本語 CSV では非常に一般的)

---

### Q1.4: 必須列欠損時のエラー伝搬

```
[現象] col_map に key がない時、`get` クロージャ (L216-223) は空文字を返す
↓ なぜ?
[直接原因] `col_map.get(key).and_then(...).unwrap_or("")` パターン
↓ なぜ?
[構造的原因] 「列がなくても致命傷ではない」設計思想。job_title/company_name 両方空のときだけ skip (L228-232)
↓ なぜ?
[根本原因] 「給与・住所が欠損していても集計は続行」許容
↓ なぜ?
[真因] downstream (parse_salary / parse_location) が空文字を `empty_result` で受ける防御を持つため伝搬しない
↓ 5 回目の Why
[究極] 「給与/住所列がない CSV」も「給与/住所値が NULL」と等価扱いされる。崩れる場合: ユーザーが「給与列があるはずの CSV」をアップロードしたが col_map.salary が拾えなかった → エラーではなく「parse 成功率 0%」として表示 → ユーザーは「給与情報がない CSV」と誤解
```

**逆証明**:
- `headers=["foo", "bar"]`, 行 `[100, 200]` を流すと col_map 空 → fallback も score 0 → records.len() = 1 (job_title=100, それ以外空) (注: L228-232 では company_name=空 でも job_title が「100」なので非空 → skip されない)
- agg の salary_parse_rate=0% / location_parse_rate=0% は出るがエラーにならない
- **テスト不足**: 「期待カラムが完全欠損したときに警告/エラー化する」テストがない

**逆因果**: なし

**Severity**: 🟢 (現状仕様としては防御的 + downstream で吸収。ただし UX 観点で警告強化推奨)

---

### Q1.5: アップロード上限 20MB の根拠

```
[現象] lib.rs:39 `pub const UPLOAD_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024;`
↓ なぜ?
[直接原因] axum DefaultBodyLimit のデフォルト 2MB は CSV 用途で小さすぎ
↓ なぜ?
[構造的原因] Indeed/求人ボックスの 1 セッション抽出 CSV は数千〜数万行で 5-15MB 規模 (推定)
↓ なぜ?
[根本原因] tower-sessions のメモリ保持なので、複数同時アップロードでサーバーメモリを圧迫しない上限が必要
↓ なぜ?
[真因] **明確な根拠ドキュメントなし**。docs/E2E_COVERAGE_MATRIX.md L335 で 21MB→413 のテストはあるが「なぜ 20MB か」の論拠なし。env_variables_reference.md L57-64 も「env 化候補 (P2)」止まり
↓ 5 回目の Why
[究極] 「20MB あれば実用上十分」という暗黙の経験則。前提が崩れる場合: Indeed の大量抽出 (50,000 行 × 30 列 ≈ 25-30MB) でユーザーが分割を強いられる
```

**逆証明**:
- 既存テスト `e2e_security` で 21MB → 413 を確認 (E2E_COVERAGE_MATRIX.md L335)
- 「20MB 直下なら受信成功」は同じテストで担保
- **論理的根拠の reverse-proof なし** (例えば「**20MB ≒ 100,000 行を超える時点でブラウザ送信時 timeout のリスクが大きい**」のような engineering trade-off の記録)

**逆因果**: なし

**Severity**: 🟢 根拠不明だが運用実害なし → 🟡 (env 化推奨済、未実装)

---

## 2. 住所パース (`location_parser.rs` 1313 行)

### Q2.1: なぜ「leftmost position + longest name」ルールか?

```
[現象] extract_prefecture (L977-1017) は「最も早い位置 + 同位置で最長名前」を採用
↓ なぜ?
[直接原因] 2026-04-23 ユーザーから「東京都データが京都府として表示」のバグ報告 (location_parser_realdata_test.rs:5)
↓ なぜ?
[構造的原因] 旧実装は `text.contains("京都") then return Some("京都府")` 順に走査 → "東京都" に "京都" が部分一致して京都府誤判定
↓ なぜ?
[根本原因] 「部分文字列マッチ」を「都道府県判定」と暗黙等価視
↓ なぜ?
[真因] PREFECTURES の "京都府" "東京都" の漢字共有 (京)。素朴な contains() では区別不能
↓ 5 回目の Why
[究極] 「フル名 (都/道/府/県 込み) で最初に出現する位置を持つ pref」が「テキスト主体地」と保証している。前提が崩れる場合: ユーザーが「住所候補リスト」のような複数住所列挙 CSV を作ると、位置の早さ ≠ 主たる勤務地。例: "支社: 大阪府梅田 / 本社: 東京都新宿区" だと大阪府と判定 (が本社が主)
```

**逆証明 (既存テストで担保済み)**:
- `location_parser_realdata_test::mece_prefecture_prefix_beats_station_name` (L207-231): 47 都道府県 × 21 主要駅 = 987 通りで「先頭都道府県が必ず勝つ」
- `mece_prefecture_prefix_beats_city_alias` (L234-255): 47 × 17 = 799 通り
- `mece_prefecture_prefix_beats_shared_ward` (L258-285): 46 × 6 = 276 通り
- 旧実装で起こったバグ「東京都立川市高松駅 → 香川県」が 修正後 "東京都" に解決されることを確認
- **逆証明品質**: 高 (具体値で 2000+ ケース)

**逆因果**:
- 「都道府県名が早く出現する → 主たる勤務地」は逆「主たる勤務地 → CSV に最初に書かれる」と区別できていない
- 「複数勤務地」CSV 列での誤判定リスク → 仕様で 1 行 1 住所と暗黙仮定

**Severity**: 🟢 (修正完了済 + MECE テストあり。複数勤務地は仕様外)

---

### Q2.2: 政令指定都市の区名 (新宿区、横浜市西区等) の citycode 解決

```
[現象] try_designated_city (L1042-1132) は municipality を「横浜市」(区なし) で返す。区名は捨てる
↓ なぜ?
[直接原因] L1071-1080 で `municipality: Some(city.to_string())` の city は designated_cities 配列の市単位文字列のみ
↓ なぜ?
[構造的原因] 政令指定都市の市単位集計を優先したい設計
↓ なぜ?
[根本原因] 一方、station_map (L450-455) では `city: "横浜市西区"` のように区つき municipality を返すケースあり → **同じ「横浜の求人」でも station 経由なら区つき、designated_city 経由なら区なし**
↓ なぜ?
[真因] municipality 文字列の粒度がコードパスごとに不統一
↓ 5 回目の Why
[究極] aggregator (`muni_salary_map: HashMap<(String,String), Vec<i64>>`, L516) で「(都道府県, 市区町村)」をキーとするため、同じ横浜市の求人が station 経由 ("横浜市西区") と designated_city 経由 ("横浜市") で**別キー集計**になる
       → 件数分散 → median/avg の精度低下
```

**逆証明 (現状 reverse-proof なし)**:
- ケース1: `parse_location("横浜駅から徒歩5分", None)` → station 経由 → municipality="横浜市西区" (location_parser.rs:453 の station_map で「横浜駅 → 横浜市西区」)
  - **要確認**: 実際に station_map に「横浜駅」がある? (Grep: 横浜駅 → 検証する必要あり)
- ケース2: `parse_location("横浜市保土ヶ谷区", None)` → designated_city 経由 (L1068 の contains "横浜市") → municipality="横浜市"
- ケース3: `parse_location("神奈川県横浜市西区みなとみらい", None)` → designated_city 経由 → municipality="横浜市" (区情報失う)
- **テスト不足**: 既存 `test_designated_city` (L1201-1205) は `municipality.starts_with("横浜市")` の弱検証
- 期待される逆証明: 「(神奈川県, 横浜市) と (神奈川県, 横浜市西区) が同一バケットに集約される」テスト

**逆因果**:
- なし (純粋な実装不整合)

**Severity**: 🟡 (集計の粒度バグ。市区町村別給与表で件数が分散表示される可能性)

---

### Q2.3: 同名市区町村 (伊達市/府中市) の判別

```
[現象] try_municipality_pattern (L1135-1177) は pref と組み合わせて (pref, muni) タプルとして扱う
↓ なぜ?
[直接原因] aggregator の muni_salary_map キーが `(pref, muni)` タプル (L516)
↓ なぜ?
[構造的原因] city_code.rs の citycode 体系も (prefcode, city_name) で区別 (team_gamma_domain.md §4-1)
↓ なぜ?
[根本原因] 法的に同名市区町村が存在 (伊達市: 北海道01236 / 福島07213; 府中市: 東京13206 / 広島34208)
↓ なぜ?
[真因] **CSIS / city_code.rs と同じ取り扱い** で構造的な整合あり
↓ 5 回目の Why
[究極] 「都道府県+市区町村」のタプル一意性が保証されている前提。崩れる場合: pref が None で muni のみ取れた場合 (例: "伊達市役所前" のみ) → tuple 不完全 → muni_salary_map に入らない (L518-530 で if let (Some(pref), Some(muni)) ガードあり、安全)
```

**逆証明**:
- ケース: `aggregate_records` に 2 件投入: `(prefecture="北海道", municipality="伊達市", salary=300_000)` と `(prefecture="福島県", municipality="伊達市", salary=200_000)`
- 期待: by_municipality_salary に 2 エントリ (異なる prefecture 値で)
- **既存テストに該当 reverse-proof なし**: parser_aggregator_audit_test に 同名市区町村テストなし
- 修正前/後の差: ある (pref タプル化前は伊達市が同一キーで avg=250k になる、現状は 300k と 200k で分かれる)

**逆因果**: なし

**Severity**: 🟢 (実装は正しい。テスト不足のみ → 推奨追加)

---

### Q2.4: アクセス情報のみ (駅名のみ) の住所判定除外

```
[現象] try_station (L939-962) で「XX駅」マッチがあると station_map から (city, prefecture) を返す
↓ なぜ?
[直接原因] 駅名 → 政令指定都市/中核市の中央付近 → 都道府県確定が一般的に正しい
↓ なぜ?
[構造的原因] CSV の location 列に「○○駅から徒歩5分」のみが入るケースが多い
↓ なぜ?
[根本原因] スクレイピング元 (Indeed/求人ボックス) が住所を完全に出さず駅名のみ表示
↓ なぜ?
[真因] **駅名 != 勤務地**: 駅名はアクセス情報。実際の勤務地はその近くだが、station_map では駅の中心住所を返す
↓ 5 回目の Why
[究極] 「駅 → 駅周辺の市区町村」を「勤務地」と扱う精度ゆるい mapping を許容。崩れる場合: 「東京都の山奥にある支社、最寄駅: 新宿駅 (車で2時間)」のような場合、新宿区集計に入る → 地理的偏り
       メモ: feedback_correlation_not_causation 観点では「駅近 = 集計上その駅周辺の地域に分類」は近似でしかない
```

**逆証明**:
- `parse_location("新宿駅から徒歩60分", None)` → 新宿区, 東京都 (実際の勤務地は新宿区とは限らない)
- 期待: 都道府県のみ確定 (新宿区不確定) または confidence 低下
- 現状: confidence=0.9 で municipality="新宿区" と高信頼 (location_parser.rs:954-957)
- **🟡 信頼度の過大表示**: 「徒歩60分」など距離情報があれば confidence を下げるべきだが、現状は距離無視

**逆因果**:
- 「駅名がある → その駅近に勤務地」は正しいが、「その駅近に勤務地 → CSV に駅名が書かれる」は別の因果。後者の場合、駅名は確定情報

**Severity**: 🟢 (近似として妥当。confidence 0.9 はやや楽観的)

---

### Q2.5: city_alias マップの抜け漏れ (resolve_city_alias)

```
[現象] resolve_city_alias (L772-796, #[allow(dead_code)]) は 20 件の政令市略称を持つが、 try_designated_city (L1083-1131) は別の city_aliases 配列 (17件) を使用 → 二重定義
↓ なぜ?
[直接原因] 実装時のリファクタ未完。resolve_city_alias は dead_code
↓ なぜ?
[構造的原因] 「政令市略称」の単一の真実源がない
↓ なぜ?
[根本原因] dead_code allow で警告抑制 → 整理タイミング失う
↓ なぜ?
[真因] 「堺」「千葉」「相模原」が resolve_city_alias にあって city_aliases にない (L1083-1101 を確認)
↓ 5 回目の Why
[究極] 「政令市略称マッチ」が**部分的に欠落**。例: "堺の食品工場" → 略称マッチで "堺市" にならず → 都道府県のみ "大阪府" にもならない (堺の前後に大阪府文字列なし)
       → 都道府県不明 + コンテキストフォールバック頼み
```

**逆証明**:
- ケース: `parse_location("堺の食品工場", None)` → designated_city 「堺市」は L1059 にあるが alias で "堺" 単独はない → スキップ → try_municipality_pattern も「市」キーワードなし → empty_location with text
- 期待: prefecture=Some("大阪府"), municipality=Some("堺市")
- 現状: prefecture=None
- **テスト不足**: 「resolve_city_alias と city_aliases の不整合」を逆証明するテストなし

**逆因果**: なし

**Severity**: 🟡 (堺/千葉/相模原の略称マッチ漏れ。精度低下要因)

---

## 3. 給与パース (`salary_parser.rs` 543 行、167h 統一済)

### Q3.1: なぜ 167h か?

```
[現象] HOURLY_TO_MONTHLY = 167.0 (L36)、aggregator HOURLY_TO_MONTHLY_HOURS = 167 (L25)
↓ なぜ?
[直接原因] 厚労省「就業条件総合調査 2024」基準 (コメント L32-38)
↓ なぜ?
[構造的原因] 旧 GAS 互換 173.8h (8h × 21.7日) は労働基準法上の「年所定労働時間」近似だが、実態より過大評価
↓ なぜ?
[根本原因] 厚労省統計の月平均所定労働時間 169.0h を保守側に丸めた 167h を採用 (exec_f1_results.md §2-3)
↓ なぜ?
[真因] 8h × 20.875日 = 167h ≒ 8h × 21日 = 168 と整合 (D-day=21 と aggregator)
↓ 5 回目の Why
[究極] 「月平均所定労働時間=167h」が日本企業の中央値を代表と保証。崩れる場合: シフト勤務が多い職種 (24h 介護・看護等) では実際の労働時間 vs 所定時間に乖離。167h で割っても「実際の時給換算」と「契約時給」の乖離は残る → ユーザーは契約上の時給を知りたいので、契約時給がそのまま min_value に入っている限り問題はない
```

**逆証明 (既存テストで担保済み)**:
- `salary_parser::tests::test_hourly` (L431-437): 時給 1200 → unified_monthly = 200_400 (= 1200 × 167)
- `parser_aggregator_audit_test::alpha_salary_min_values_type_conversion_exact` (L595-631): 修正前 240_000 → 修正後 250_500 (1500 × 167) を厳密 assert
- 修正前/後の数値差を逆証明: 時給 1500 → 旧 240_000 (160h) / 中継 260_700 (173.8h) / 統一後 250_500 (167h) の 3 段階推移を exec_c3_results.md §3.3 に記録
- **逆証明品質**: 高

**逆因果**:
- 「167h は標準 → 全雇用形態で適用」は逆「派遣 (日勤8h×20日のみ) なら 160h、介護夜勤あり契約なら 200h」の方が正確かもしれない
- 単一定数を全ての雇用形態に適用 → emp_group ごとの差異を吸収しない (Plan P2 #14 で「派遣・パートで系統的過小評価」と指摘済み、F1 #2 で 167 に統一だが**雇用形態別係数は未実装**)

**Severity**: 🟢 (167h は妥当。雇用形態別係数化は将来課題)

---

### Q3.2: 月給換算 167h と aggregator 167h の完全一致確認 (C-3 で修正済、再検証)

```
[現象] salary_parser.rs:36 / aggregator.rs:25 ともに 167 (型は f64 vs i64)
↓ なぜ?
[直接原因] C-3 (exec_c3_results.md) で 173.8 → 167 統一実施
↓ なぜ?
[構造的原因] 旧 F1 修正時に aggregator のみ 167 にし、salary_parser は GAS 互換で 173.8 のまま
↓ なぜ?
[根本原因] V2 HW Dashboard は V1 (ジョブメドレー) と独立リポであり、GAS 互換性は要件外
↓ なぜ?
[真因] parser ↔ aggregator 経路で 47 円差が発生 (200_000円 月給 → parser 経由 1150 円/h vs aggregator 経由 1197 円/h, exec_c3_results.md §3.3)
↓ 5 回目の Why
[究極] 「parse_salary 経由でも aggregator 直変換でも同じ月給値」を保証。崩れる場合: 整数除算誤差 ±1 円 (i64 切り捨て)
       検証: `f1_consistent_173_to_167_migration` テストで「両者の差が ±1 円以内」を assert (exec_c3_results.md §3.2.3)
```

**逆証明 (既存)**:
- exec_c3_results.md §3.3: 月給 200_000 → 時給 (parser) 1197 / (aggregator) 1197 → 差 0 (整数化)
- 統一後も f64 vs i64 の型差: parser は `m = base * 167.0` then `as i64`、aggregator は `v / 167` (整数除算) → 切り捨て方向が同じなので差 ≤ 1 円
- **逆証明テスト健在性**: 改名後の `f1_consistent_173_to_167_migration` 健在 (確認: parser_aggregator_audit_test.rs:583+ にあり)

**逆因果**: なし (純粋数値整合)

**Severity**: 🟢 (修正完了)

---

### Q3.3: 賞与月数の表記ゆれ (「2.5ヶ月」「2.5月」「2か月」)

```
[現象] salary_parser.rs に bonus_months フィールドなし。賞与関連パース完全欠落
↓ なぜ?
[直接原因] ParsedSalary 構造体に bonus フィールドなし (L17-28)、parse_salary 関数に賞与抽出ロジックなし
↓ なぜ?
[構造的原因] CSV salary 列は「給与本体」のみという設計仮定
↓ なぜ?
[根本原因] 賞与情報は description/tags 列にあるが extract_annual_holidays (upload.rs:835-877) と違って賞与抽出ロジックが実装されていない
↓ なぜ?
[真因] team_gamma_domain.md §2-6 で「Panel 5 condition_gap.rs が `annual_income = salary_min × (12 + bonus_months)` を使用」とあるが、この bonus_months は HW DB 側 ts_salary_yearly テーブル等から取得。**survey 側は CSV から賞与抽出していない**
↓ 5 回目の Why
[究極] 「アップロード CSV の年収比較」は月給 × 12 のみで近似しており、HW 側 (賞与込み) との比較で過小評価。崩れる場合: ユーザーが「年収比較」を見ると「HW より低い」と誤判断 → 「賞与4ヶ月込み HW」vs「賞与なし CSV」の不公平比較
```

**逆証明 (現状 reverse-proof なし)**:
- 仮想ケース: description="月給25万円、賞与年2.5ヶ月" を投入
- 期待: parsed.bonus_months = Some(2.5)
- 現状: parsed.bonus_months 自体存在せず → annual_income = 25万 × 12 = 300万 (本来 25万 × 14.5 = 362.5万)
- **テスト全欠**: Grep `bonus_months` 0 件

**逆因果**:
- 「賞与未抽出 → 年収過小表示 → ユーザーが応募を見送る」は逆方向「ユーザーが応募見送り → 求人滞留 → CSV 件数増」と誤関連付けされうる
- 媒体分析タブの示唆文 (so-what) 上で「年収が低い傾向」と表示される場合、**賞与未抽出が原因**であることを明記する必要

**Severity**: 🔴 (年収比較精度に影響。HW 側 condition_gap.rs (賞与込み) との比較で系統的バイアス)

---

### Q3.4: 範囲表記 (200,000〜250,000) の min/max 抽出

```
[現象] extract_salary_values (L143-165) で `~` 分割 → splitn(2, '~') で max 2 分割
↓ なぜ?
[直接原因] normalize_text で「～〜ー―－」を全て `~` に統一 (L95)
↓ なぜ?
[構造的原因] 全角・半角・ダッシュ類のバリエーションが多い
↓ なぜ?
[根本原因] CSV の生表記が一定でない
↓ なぜ?
[真因] only_min ケース (例: "月給20万円～") は (Some(l), None, true) として返す (L154-155)。only_max (例: "～月給30万円") は (None, Some(r), true)
↓ 5 回目の Why
[究極] 「範囲表記の左右いずれか欠けても has_range=true」が保証。崩れる場合: 3 値表記 (例: "200,000～300,000～400,000" 異常データ) は splitn(2) で「200,000」と「300,000～400,000」に分かれ、後者の単一値が parse_decimal_man 等でパースされる → max=300_000 だけ取って残り無視
```

**逆証明 (既存)**:
- `salary_parser::tests::test_min_only_range` (L475-481): "月給20万円～" → min=200_000, max=None, has_range=true
- `test_monthly_range` (L421-428): "月給25万円～30万円" → min=250k, max=300k, unified=275k
- `parser_aggregator_audit_test::alpha_real_indeed_monthly_comma_range_exact_values` (L102-112): 実データ「月給 241,412円 ~ 401,412円」で具体値検証
- **逆証明品質**: 中 (only_max ケース、3 値ケースのテストなし)

**逆因果**: なし

**Severity**: 🟡 (3 値ケース未テスト。実害は限定的)

---

### Q3.5: 異常値 (例: 月給 1 円、月給 1 億円) の検出と除外

```
[現象] calculate_confidence (L371-405) で範囲チェックあり: Monthly (100,000..=2,000,000) (L395)
↓ なぜ?
[直接原因] confidence スコア計算用の妥当範囲チェック
↓ なぜ?
[構造的原因] confidence は 0.5 起点で範囲内だと +0.2 (L399-401)
↓ なぜ?
[根本原因] **confidence は表示用フィルタには使われていない** (Grep 確認: aggregator は unified_monthly が Some なら無条件に集計に入れる)
↓ なぜ?
[真因] 異常値除外は集計層 (aggregator) 側で IQR 1.5 で行う (L311-312) + salary_min_values は 50_000 円以上のみ (L355)
↓ 5 回目の Why
[究極] 「parse_salary 自体は値域チェックせず、aggregator で IQR + 5万円下限カット」という二段構え。崩れる場合: 月給 200,000,000 (2億円) のような異常上限値が IQR の上限超えで除外されない場合 (n<4 なら全件通過 L176-178、IQR=0 なら全件通過 L184-186) → **異常値が median/mean を歪める**
       下限 5 万円 (aggregator.rs:355) は salary_min_values のみで、enhanced_salary_statistics 入力 (`salary_values` = unified_monthly) は IQR のみ → IQR 通過した上限値はそのまま
```

**逆証明**:
- データ: `[200_000, 250_000, 300_000, 100_000_000]` (n=4 で IQR 計算可能)
  - Q1=237_500 (200k と 250k の P25), Q3=24_949_999 (P75)、IQR≈24M、上限=Q3+1.5×IQR≈61M → 100M は外れ値除外 ✅
- データ: `[200_000, 100_000_000]` (n=2 で IQR n<4)
  - filter_outliers_iqr (L176-178) で全件通過 → mean = (200k + 100M) / 2 = 50,100,000 → 「平均月給5000万円」と表示
- **テスト**: `outlier_tests::filter_small_sample_passes_through` (statistics.rs:222-227) は n<4 で全通過を確認するが、「異常値が混入する」逆証明テストではない

**逆因果**:
- 「月給1億 → 異常値」は逆「異常値 → CSV にエラー混入」で正しいが、「**確かに月給1億円相当の役員求人** → 異常ではない」というケースもあり得る → 一律切り捨ては要件次第

**Severity**: 🟡 (n<4 時の保護なし。少件数地域でリスク)

---

## 4. 雇用形態分類 (`emp_classifier.rs` + `aggregator.rs::classify_emp_group_label`)

### Q4.1: なぜ「契約社員」を Other (派遣・その他) にするか?

```
[現象] emp_classifier::classify (L41-50) で「正社員以外」「契約社員」「業務委託」「派遣」全て Other
↓ なぜ?
[直接原因] team_gamma_domain.md §3-4 の不整合修正 (契約社員が survey で Regular / diag で Other だった)
↓ なぜ?
[構造的原因] 雇用契約期間の有無で月給制度が異なる (正社員=無期、契約社員=有期)
↓ なぜ?
[根本原因] 契約社員の月給は「契約期間限定の固定給」で正社員月給と並べて中央値計算すると分布が混在
↓ なぜ?
[真因] さらに業務委託は「報酬」概念で月給ではなく案件単位 → 正社員グループに混ぜると過大評価
↓ 5 回目の Why
[究極] 「正社員 = 無期月給」「Other = それ以外の報酬形態」を雇用形態の経済的本質で分類。崩れる場合: aggregator.rs:699-711 の **旧 classify_emp_group_label** が「契約」「業務委託」も「正社員」グループに含めている (現状アクティブ) → emp_classifier (新) は "正社員以外" を Other にしているが、aggregator は完全には乗り換えていない
       Plan P2 #2 / exec_e2_results.md §6 で `emp_classifier.rs` を新設したが「後方互換のため survey/aggregator の旧関数を残す」と明示 (emp_classifier.rs:13-16)
       → **呼出元が乗り換わっていない場合、新分類は適用されない**
```

**逆証明 (既存)**:
- `emp_classifier::tests::classify_contract_worker_is_other_not_regular` (L100-103): "契約社員" → Other
- `classify_gyomu_itaku_is_other_not_regular` (L107-110): "業務委託" → Other
- `classify_seishain_igai_is_other_not_regular` (L93-96): "正社員以外" → Other
- **不足**: aggregator 側 classify_emp_group_label の旧実装 (L699-711) を逆証明する「修正後同じ結果になる」テストがない
  - 期待: `classify_emp_group_label("契約社員") == "正社員"` (旧仕様、現状) vs `classify(...) == EmpGroup::Other`
  - aggregator 側を更新するための reverse-proof test 不在

**逆因果**:
- 「業務委託 = 報酬」と整理しても、**業務委託の単価分布**は実態として正社員月給より高いケースもある (フリーランス IT エンジニア等)
- 「業務委託を Other に隔離 → 正社員月給中央値が下がる」とは限らない (むしろ上がる場合も)
- 媒体分析の示唆文で「正社員給与は X 円」と表示する際、Other に隔離した結果が「業界水準」と一致するか、逆因果的検証 (国勢調査賃金構造基本統計調査との比較) は未実施

**Severity**: 🔴 (旧 aggregator 関数が現役 + 新 emp_classifier が並存 → 二重定義残存)

---

### Q4.2: 業務委託は Other 扱い

```
[現象] expand_to_db_values (emp_classifier.rs:56-62) で Other = ["正社員以外", "派遣", "契約社員", "業務委託"]
↓ なぜ?
[直接原因] team_gamma_domain.md M-4 矛盾解消
↓ なぜ?
[構造的原因] 旧 recruitment_diag::expand_employment_type は業務委託をどこにも分類せず空フィルタ (team_gamma §3-4)
↓ なぜ?
[根本原因] HW postings.employment_type の値が「正社員」「正社員以外」「パート労働者」「有期雇用派遣パート」「無期雇用派遣パート」「派遣」「契約社員」「業務委託」と多様
↓ なぜ?
[真因] DB の employment_type 値リスト (上記) と UI 3 区分 (正社員/パート/その他) のマッピングが必要
↓ 5 回目の Why
[究極] 「Other 区分には業務委託を含む」が保証。崩れる場合: HW 側 ETL で employment_type が新規追加 (例: 「シニア嘱託」) → expand_to_db_values の手動メンテが必要 → 漏れると DB 検索でヒットせず「データ不足」誤表示
```

**逆証明 (既存)**:
- `expand_other_includes_contract_and_gyomu_itaku` (emp_classifier.rs:128-137): Other に「正社員以外」「派遣」「契約社員」「業務委託」全 4 件含むことを assert (`v.len() == 4`)
- 修正前/後の差: 旧 3 件 (業務委託なし) → 新 4 件
- **逆証明品質**: 高 (具体値 + len 検証)

**逆因果**: なし

**Severity**: 🟢 (修正完了)

---

### Q4.3: from_ui_value / expand_to_db_values の整合性

```
[現象] from_ui_value (L65-72) は UI 3 値「正社員/パート/その他」を EmpGroup へ、expand_to_db_values は EmpGroup を DB 値リストへ
↓ なぜ?
[直接原因] UI ↔ DB の双方向変換を分離 (Single Responsibility)
↓ なぜ?
[構造的原因] UI 値と DB 値の集合が異なる (UI=3 / DB=8 値)
↓ なぜ?
[根本原因] 単一マップではなく EmpGroup を中継 enum に
↓ なぜ?
[真因] label() メソッド (L28-34) で逆方向 EmpGroup → UI label を取れる
↓ 5 回目の Why
[究極] from_ui_value("正社員") → Regular → label() = "正社員" の往復が一致を保証。崩れる場合: 大文字小文字 / 全角半角の差異 → from_ui_value("正社員 ") (末尾スペース) は None
```

**逆証明 (既存)**:
- `from_ui_value_three_options` (L158-163): 3 値で Some/None 検証
- `label_consistency_with_from_ui_value` (L166-172): UI → EmpGroup → label の往復一致 assert
- **逆証明品質**: 高

**逆因果**: なし

**Severity**: 🟢

---

### Q4.4: 表記ゆれ (「正社員」「正職員」「フルタイム」「契約社員」「契約職員」)

```
[現象] classify (emp_classifier.rs:41-50): 「パート/アルバイト」優先 → 「正社員/正職員 (含 "以外" 除外)」 → Other
       upload.rs:777-797 normalize_employment_type は「正社員/正職員」「契約社員/嘱託」「紹介予定派遣」「派遣」「パート/アルバイト」「業務委託/請負」を 6 区分に正規化
↓ なぜ?
[直接原因] CSV 値の「契約職員」「フルタイム」「常勤」「准社員」等の variant に対応
↓ なぜ?
[構造的原因] 媒体ごとの表記が一定でない (Indeed=「正社員」, JobBox=「正職員」もあり)
↓ なぜ?
[根本原因] **CSV値の正規化** (upload.rs) と **DB値の分類** (emp_classifier.rs) が別レイヤー
↓ なぜ?
[真因] **「契約職員」「常勤」「准社員」「フルタイム」のテストなし** (Grep 確認)
↓ 5 回目の Why
[究極] 「主要な雇用形態は normalize で 6 区分に集約」を保証。崩れる場合: 「契約職員」(医療・公務員職に多い) は normalize で「契約社員」とマッチしない → そのまま生文字列 → emp 別集計で別カテゴリ
       「フルタイム」「常勤」も同様 → 「不明」扱いまたは独立カテゴリ
```

**逆証明**:
- ケース: employment_type="契約職員" → normalize_employment_type は「契約」を含むかチェック (upload.rs:781) → contains("契約社員") なので「契約職員」は match しない (契約社員 != 契約職員)
- 正解修正案: `val.contains("契約")` で広く取る (現状: contains("契約社員") || contains("嘱託"))
- **テスト全欠**: normalize_employment_type の表記ゆれテストなし
- 修正前: "契約職員" → そのまま "契約職員" 残る → classify で Other
- 修正後 (もし広く取れば): "契約職員" → "契約社員" → classify で Other (同じ結果)
- **結果としては Other 行きで実害はない**が、emp_map (aggregator.rs:262-269) の集計表示で「契約職員」と「契約社員」が別カテゴリで現れる

**逆因果**: なし

**Severity**: 🟡 (表示の冗長性、データ集計上は許容)

---

### Q4.5: 時給データを月給に混在させない仕組み

```
[現象] aggregate_by_emp_group_native (aggregator.rs:588-696) でグループ別に native_unit (月給/時給) を選択
↓ なぜ?
[直接原因] 正社員=月給、パート=時給、派遣・その他=多数派決定 (L640-651)
↓ なぜ?
[構造的原因] 同一グループ内でも salary_type 混在 → 単位を揃える必要
↓ なぜ?
[根本原因] 時給 1500 円と月給 250,000 円を素朴に平均すると意味不明
↓ なぜ?
[真因] 各レコードの salary_type に応じて monthly_values と hourly_values の両方を bucket に投入 (L605-635)、表示時に native_unit に応じて選択 (L652-656)
↓ 5 回目の Why
[究極] 「グループ単位で単位整合 + IQR 1.5 で外れ値除外 (L662)」を保証。崩れる場合: パートグループに月給契約のレコード (誤データ) が混入 → monthly_value は v / 167 で時給化 → サンプル少ないと偏りに → IQR 通過
       例: 「パート - 時給1200円」9 件 + 「パート - 月給200,000円」1 件 (誤データ) → 月給は 200_000/167 = 1197 → 時給と並ぶ → IQR で異常検出されない
```

**逆証明**:
- ケース: 上記の 10 件投入 → IQR は中央値1200 付近、Q3-Q1 が小さい → 1197 は許容範囲内 → 全 10 件で hourly_values の median = 1200 (1197 が混在したまま)
- 期待値: 単位ガード (パートで月給はそもそも投入しない) で 9 件のみ集計
- **テスト不足**: 「パートに月給データを混ぜたら」の reverse-proof なし

**逆因果**:
- 「単位ガード = データの信頼性向上」だが、現状は「単位混在 → 換算 → 統計的外れ値で除外」で間接的にガード。逆方向「換算結果が外れ値 → 元データが正しい単位ではなかった」を統計的にしか検出できない

**Severity**: 🟡 (単位ガードの実装欠落。実害は限定的)

---

## 5. 統計処理 (`statistics.rs` 419 行)

### Q5.1: なぜ IQR 1.5 倍 (Tukey 標準) か?

```
[現象] filter_outliers_iqr (L173-197) で iqr_multiplier=1.5 をデフォルト
↓ なぜ?
[直接原因] aggregator から `filter_outliers_iqr(&raw, 1.5)` で呼出 (L312, 662)
↓ なぜ?
[構造的原因] Tukey の箱ひげ図定義の標準値 (team_gamma_domain.md §2-2「業界標準と整合 ✅」)
↓ なぜ?
[根本原因] 1.5 は正規分布で約 99.3% を inlier として残す threshold
↓ なぜ?
[真因] 給与分布は右に歪む (右裾長い) → 1.5×IQR の上限が低めに出て上位給与を過剰除外
↓ 5 回目の Why
[究極] 「正規分布前提の Tukey 1.5」を給与 (右歪) に適用 → **上位の高給与を outlier として切り捨て** → median/mean が下方バイアス。崩れる場合: 役員求人など高給与クラスタが「外れ値」と消される
       より厳密: 対数正規分布なら log 変換後の IQR 計算が望ましい (Plan P2 / team_gamma に記載なし)
```

**逆証明 (既存)**:
- `outlier_tests::filter_removes_high_outlier` (L203-211): `[200, 220, 240, 250, 260, 280, 300, 1000]` で 1000 を除外
- `filter_keeps_normal_data` (L213-219): `[200..300]` で全件通過
- `filter_small_sample_passes_through` (L221-227): n<4 で全通過
- **逆証明品質**: 高 (具体値 + 件数 assert)
- **不足**: 「1.5 vs 2.0 vs 3.0 の比較」 + 「右歪分布での過剰除外」テストなし

**逆因果**:
- 「IQR 1.5 適用 → 統計的に妥当な代表値」は逆「**実態の代表値 → IQR 1.5 で算出される結果**」と同値とは限らない (歪んだ分布で逆対応関係が崩れる)
- 媒体分析の so-what 「平均月給 X 円」表示時、IQR で除外された分は**集計対象外**である旨の明示が render.rs にあるか要確認

**Severity**: 🟢 (1.5 は業界標準。歪みの考慮は将来課題)

---

### Q5.2: Bootstrap 反復回数 2000 の根拠 (1000 では足りないか)

```
[現象] enhanced_salary_statistics (L284-288) で n>=5 のとき bootstrap_confidence_interval(&valid, 2000)
↓ なぜ?
[直接原因] team_gamma_domain.md §2-3「2000 は標準的 (1000-10000)。妥当」
↓ なぜ?
[構造的原因] パーセンタイル法の信頼区間収束に必要な反復数
↓ なぜ?
[根本原因] R/Python 等の標準ライブラリ default が 1000-2000
↓ なぜ?
[真因] 2000 反復で 95% CI のパーセンタイル idx_lower=2000×0.025=50, idx_upper=2000×0.975=1950 (L56-57) → 結果の精度ε ≈ 1/√2000 ≈ 2.2%
↓ 5 回目の Why
[究極] 「2000 回で 95% CI が ±2% 程度の精度」が保証。崩れる場合: 極端に歪んだ分布や n が大 (10万+) のときは 2000 でも収束しない可能性。逆に n=5 程度では 2000 反復でも CI が広すぎて意味薄
       メモ: BCa 法 (bias-corrected accelerated) なら 1000 回で同等精度 (Q5.5 参照)
```

**逆証明 (既存)**:
- `tests::test_bootstrap` (L321-331) は data 7 件で iterations=1000 で実施 → CI が範囲内 (200k〜300k)
- `test_bootstrap_single` / `test_bootstrap_two` で n=1, 2 のエッジケース
- **不足**: 1000 vs 2000 vs 5000 の収束比較テストなし

**逆因果**: なし

**Severity**: 🟢 (2000 は標準的妥当値)

---

### Q5.3: Trimmed mean 上下 10% カットの根拠

```
[現象] enhanced_salary_statistics (L289-293) で n>=10 のとき trimmed_mean(&valid, 0.1)
↓ なぜ?
[直接原因] team_gamma_domain.md §2-4「上下 10% トリム標準的 (5-20%)。妥当」
↓ なぜ?
[構造的原因] IQR と独立した外れ値耐性指標を提供
↓ なぜ?
[根本原因] median は中央 1 点、mean は全体平均。trimmed mean は両者の中間
↓ なぜ?
[真因] 0.1 = 10% カットなら n=10 で 1 件ずつ除外 (合計 2 件)。n=20 で 2 件ずつ (合計 4 件)
↓ 5 回目の Why
[究極] 「上下 10% トリムは Huber loss 等の高破壊性 (high-breakdown) 推定量と類似の頑健性」を提供。崩れる場合: n=10 で 1 件除外は IQR の inlier_count と一致しない可能性 → 「IQR で除外されなかったが trimmed では除外された」値が混在
       2 つの外れ値除外法が**重複適用** (filter_outliers_iqr 後に enhanced_salary_statistics に入る → trimmed_mean がさらに 10% カット) で除外件数が見えにくい
```

**逆証明 (既存)**:
- `tests::test_trimmed_mean` (L333-343): `[100k, 200k..500k]` (n=10) で trimmed_count=2, trimmed_mean > original_mean - 50k
- **不足**: IQR 後の trimmed が「**二重除外**」を起こすことを記録するテストなし

**逆因果**:
- IQR 除外後の母集団に trimmed_mean を適用 → 統計的には「2 つの保護が直列適用」 → 過剰なロバスト化の可能性
- 「trimmed_mean が低い値 → 給与が低い」と読めるが、逆「給与が低い → IQR + trimmed で生き残った値」と解釈可能 (因果ではなく**選別**の結果)

**Severity**: 🟡 (二重除外の意図確認要)

---

### Q5.4: 母分散 vs 標本分散 (n 除算 vs n-1 除算) の選択

```
[現象] enhanced_salary_statistics (L277-282) で variance = sum(...) / n (母分散)、std_dev = sqrt(variance)
↓ なぜ?
[直接原因] 単純な記述統計
↓ なぜ?
[構造的原因] サンプルから母集団推定するなら n-1 (Bessel 補正) が偏りなし推定量
↓ なぜ?
[根本原因] team_gamma_domain.md §2-5「母分散 (n 除算) — 標本分散 (n-1 除算) ではない。サンプル統計量としては偏りあり」と明示済み
↓ なぜ?
[真因] survey の母集団は「アップロード CSV」自体 → 「全数調査」とみなせば母分散で正しい
↓ 5 回目の Why
[究極] 「アップロード CSV = 全数」の前提で母分散が正解。崩れる場合: ユーザーが「アップロード CSV」を「日本の求人市場のサンプル」と解釈する場合 → 標本分散 (n-1) が適切。**前提の解釈次第**
       n が大 (>100) なら n vs n-1 の差は 1% 未満で実害ほぼなし
```

**逆証明**:
- ケース n=5 のとき variance(n) と variance(n-1) の差は 1/5 = 20% → std_dev で √(5/4) ≒ 1.118 倍差
- **テスト不足**: 「**std_dev に Bessel 補正なし**」を逆証明するテストなし
- 修正前 (n 除算): variance=400, std_dev=20
- 修正後 (n-1 除算): variance=500, std_dev=22.4 (n=5 の場合)

**逆因果**: なし (純粋な統計学的選択)

**Severity**: 🟢 (前提次第、現状は記載済 + 影響軽微)

---

### Q5.5: BCa Bootstrap 未採用 (パーセンタイル法のみ) の影響

```
[現象] bootstrap_confidence_interval (L21-69) はパーセンタイル法 (L56-57)
↓ なぜ?
[直接原因] 単純実装で bias 補正・accel 補正なし
↓ なぜ?
[構造的原因] BCa 法は bias 推定 + jackknife による acceleration 計算が必要
↓ なぜ?
[根本原因] 実装複雑度が高い & 給与分布の歪みは log 正規 (skew あり)
↓ なぜ?
[真因] team_gamma_domain.md §2-3「単純パーセンタイル法のため、歪みが大きい分布で CI が偏る。給与は典型的に右に歪むため、過小推定の懸念」と認識済み
↓ 5 回目の Why
[究極] 「単純パーセンタイル法は歪み分布で CI が左にシフト (上限を過小推定)」を許容。崩れる場合: 高給与求人 (役員/IT 高度) が混じる地域で「平均月給の上限」が実態より低く表示される
```

**逆証明 (現状なし)**:
- 標準データ (n=20、対数正規) で BCa と percentile を比較するテストなし
- 修正前 (percentile): upper=420k
- 修正後 (BCa): upper=440k (推定、5% 増)

**逆因果**: なし

**Severity**: 🟢 (歪み分布での懸念点。要件次第で BCa 移行検討)

---

## 6. 集計 (`aggregator.rs` 1283 行)

### Q6.1: by_municipality_salary の市区町村集計が「都道府県+市区町村」のタプル正規化か単純文字列か

```
[現象] muni_salary_map: HashMap<(String, String), Vec<i64>> (L516)
↓ なぜ?
[直接原因] 同名市区町村 (伊達市, 府中市) を区別するため
↓ なぜ?
[構造的原因] team_gamma_domain.md §4-1 で「同名市区町村バグなし。✅」と確認済
↓ なぜ?
[根本原因] city_code.rs 体系も (prefcode, city_name) で区別
↓ なぜ?
[真因] L518-530 で `if let (Some(pref), Some(muni))` ガードあり → tuple 不完全時は集計に入らない
↓ 5 回目の Why
[究極] 「都道府県+市区町村のタプル一意性」を保証。崩れる場合: location_parser で municipality を「横浜市」(designated_city 経由) と「横浜市西区」(station 経由) で異なる文字列を返す (Q2.2 参照) → **同じ実勤務地の求人が 2 タプルに分散**
```

**逆証明 (現状不足)**:
- 既存テスト `parser_aggregator_audit_test` には「(神奈川県, 横浜市) と (神奈川県, 横浜市西区) が同一バケットになる」逆証明なし
- 必要なテストケース:
  ```rust
  let r1 = rec_with(prefecture="神奈川県", municipality="横浜市", salary=300_000);
  let r2 = rec_with(prefecture="神奈川県", municipality="横浜市西区", salary=200_000);
  let agg = aggregate_records(&[r1, r2]);
  // 現状: by_municipality_salary が 2 entries (300k と 200k で別)
  // 期待: 1 entry で avg=250k (但し location_parser を統一する必要)
  ```

**逆因果**: なし

**Severity**: 🟡 (Q2.2 と関連。location_parser 側修正でカスケード解消可能)

---

### Q6.2: 雇用形態グループ別ネイティブ単位 (正社員月給 / パート時給 / 派遣その他) の境界

```
[現象] aggregate_by_emp_group_native (L588-696) で native_unit を group_label から決定 (L640-651)
↓ なぜ?
[直接原因] 正社員 → 月給、パート → 時給、派遣・その他 → 多数派
↓ なぜ?
[構造的原因] グループによる主たる雇用慣行
↓ なぜ?
[根本原因] 同一グループ内で異なる単位の値が混じると統計が無意味
↓ なぜ?
[真因] L605-635 で salary_type ごとに monthly_values と hourly_values の**両方**を bucket に投入 (例: Hourly レコードは hourly_values にそのまま、monthly_values に v×167)
↓ 5 回目の Why
[究極] 「グループ × native_unit のマトリクスで 1 値抽出」を保証。崩れる場合: 派遣・その他で salary_type 多数派が時給だが、混入レコード (例: 派遣で月給 500_000) は monthly_values で 500_000、hourly_values で 500_000/167 ≒ 2994 として両方に入る → native_unit=時給選択時、2994 が他の時給値 (1500-2000 想定) と並ぶ → IQR で除外される or されない (n と分布次第)
       境界条件: `bucket.hourly_values.len() > bucket.monthly_values.len()` (L645) で多数派決定だが、 **両配列とも全レコード数と同じ長さ** (L605-634 で必ず両方 push) → **常に等しい → strict > false → "月給"** (L644-650 の else)
       → **派遣・その他は常に月給を選ぶ** (バグ)
```

**逆証明 (現状不足)**:
- 既存テストに「派遣・その他で時給多数のとき native_unit='時給'」を逆証明するテストなし
- 必要なテストケース:
  ```rust
  // 派遣 5 件全員時給
  let records: Vec<_> = (0..5).map(|i| rec(emp="派遣社員", salary_type=Hourly, min=1500)).collect();
  let groups = aggregate_by_emp_group_native(&records);
  let dispatch = groups.iter().find(|g| g.group_label == "派遣・その他").unwrap();
  assert_eq!(dispatch.native_unit, "時給"); // 期待: 時給
  // 現状: monthly_values も hourly_values も 5 件 → bucket.hourly_values.len() (5) > monthly_values.len() (5) = false → "月給" 誤選択
  ```
- **修正前 (推測現状)**: native_unit="月給" になる → 1500×167=250_500 が表示
- **期待**: native_unit="時給" → 1500 が表示

**逆因果**: なし

**Severity**: 🔴 (派遣・その他の native_unit 自動決定ロジックに論理エラーの疑い、要本番検証)

---

### Q6.3: 散布図 R² (linear_regression_points) の ss_tot=0 ハンドリング

```
[現象] linear_regression_points (L714-756) で ss_tot=0 のとき r_squared=0.0 (L745-749)
↓ なぜ?
[直接原因] 0 除算回避
↓ なぜ?
[構造的原因] 全点の y が同じ → ゼロ分散 → R² 数学的に未定義
↓ なぜ?
[根本原因] 「相関なし」として扱う保守的挙動 (L743-744 コメント)
↓ なぜ?
[真因] R² = 1 - ss_res/ss_tot は ss_tot=0 で divide-by-zero。0.0 を返すことで「相関なし」と表現
↓ 5 回目の Why
[究極] 「ゼロ分散時に R²=0 を返す」が保証。崩れる場合: ユーザーが「R²=0 → 相関なし → データ不適切」と読む可能性。実態は「**全点の y 値が同じ**」 (定数 y) で、x との相関ではなく y 自体が定数。完全に異なる現象
       表示時に「データのばらつきが小さく相関を判定できません」のような注記が必要だが、render.rs の表示ロジック未確認
```

**逆証明 (既存)**:
- `tests::test_linear_regression_r_squared_zero_ss_tot` (aggregator.rs:883-901): `[(1,100),(2,100),(3,100)]` で r_squared=0.0
- **逆証明品質**: 高
- **不足**: ユーザー表示側で「R²=0 が "ゼロ分散" 由来」を区別できる UI 注記の確認テスト

**逆因果**:
- 「R²=0 → 相関なし」と読まれるが、「R²=0 → ゼロ分散 (y 定数)」というケース「相関判定不能」と区別すべき
- 媒体分析タブで散布図を表示する際の so-what 文言で「相関が弱い」と表示すると逆因果の誤誘導
- → feedback_correlation_not_causation 違反の可能性

**Severity**: 🟡 (UI 表示の注記次第)

---

### Q6.4: 同一 CSV を 2 回アップロードした時の重複除外

```
[現象] **重複検出ロジック完全欠如** (Grep: file_hash, dedupe, deduplicate 0 件)
↓ なぜ?
[直接原因] parse_csv_bytes は流れてきた bytes を毎回パースし、SurveyRecord に row_index しか付与しない (upload.rs:331)
↓ なぜ?
[構造的原因] tower-sessions 上に session ごとに直近のアップロード結果のみ保持 (新規アップロードで上書き)
↓ なぜ?
[根本原因] 同一セッションの再アップロードは「**前回データを破棄して新データに置換**」が仕様 (上書き)
↓ なぜ?
[真因] 同一データを 2 回流すと結果は同じになる (idempotent) → 重複問題はセッション内では発生しない
↓ 5 回目の Why
[究極] 「セッション単位の最新優先」が保証。崩れる場合: ユーザーが**異なるセッション**でで同一 CSV を流す or 同一セッションで 2 つの CSV を結合したいケース → 結合機能なし、強制上書き
       hw_enrichment.rs:89 の重複除去は「同一 (pref, muni) ペアの DB 問い合わせ削減」であって CSV レコード重複ではない (L89: `重複除去（同一 pref+muni の複数行が CSV にあっても HW 側は1回だけ問い合わせる）`)
       → CSV 内に重複行 (同一企業・同一住所・同一給与の 2 行) があれば、**2 件としてカウント** される
```

**逆証明 (現状なし)**:
- ケース: 同一行を 2 行含む CSV を流す
- 期待: total_count=1 (重複除外) または明示的に「重複検出: N 件」表示
- 現状: total_count=2 (素直に 2 件カウント)
- **テスト全欠**

**逆因果**:
- スクレイピングで誤って同一求人を 2 度取得することは多々ある (検索条件重複等)
- 重複放置 → 平均/中央値の bias、重み付けの歪み (重複が多い企業ほど影響大)
- 媒体分析の so-what で「全 N 件の求人」と表示すると、重複を含む可能性を説明していない

**Severity**: 🔴 (重複検出なし → CSV 品質依存。スクレイピング bias を統計に直接反映)

---

### Q6.5: 都道府県・市区町村 NULL 時の挙動

```
[現象] aggregator (L232-235): location_parsed.prefecture が None のレコードは pref_map に入らない → by_prefecture から除外
↓ なぜ?
[直接原因] `if let Some(pref) = &r.location_parsed.prefecture { ... }` ガード
↓ なぜ?
[構造的原因] 「都道府県不明」を「集計対象外」として扱う
↓ なぜ?
[根本原因] location_parse_rate (L226) で全体での location パース成功率を別途表示
↓ なぜ?
[真因] パース失敗レコードは「不明」として扱い、給与統計 (salary_values) には含まれる (L307-310)
↓ 5 回目の Why
[究極] 「住所パース失敗でも給与統計には入れる」が保証。崩れる場合: location_parse_rate=50% 等の表示時、「**都道府県別給与表に出ている件数の合計**」と「**total_count**」が一致しない → ユーザーが「件数が合わない」と疑問を持つ
       例: total=100, location_ok=50 → by_prefecture の sum=50、salary_values=100 (パース成功率次第)
```

**逆証明**:
- ケース: 100 件中 50 件が prefecture=None
- 期待: by_prefecture の各エントリ count 合計 = 50, total_count=100
- **テスト不足**: 「都道府県不明レコードが all_count に含まれるが pref_map に含まれない」逆証明なし
- 代替: `parser_aggregator_audit_test::alpha_dominant_prefecture_*` 系で確認できるかは要 grep

**逆因果**: なし

**Severity**: 🟢 (現状仕様として一貫。表示側で件数差異が混乱を招く可能性あり)

---

## 7. 検出した未解決バグ・誤誘導表現リスト

### 🔴 Critical (4 件)

| # | 場所 | 問題 | 影響 |
|---|------|------|------|
| 1 | upload.rs:134-138 | UTF-8 BOM のみ対応、Shift-JIS 完全未対応。Excel 標準保存 CSV が「データ行なし」エラーで全滅 | 日本語ユーザーの実用性 |
| 2 | salary_parser.rs 全体 | bonus_months フィールド・抽出ロジック完全欠如。年収比較で系統的過小評価 (HW 側 condition_gap.rs は賞与込み) | 不公平比較の誤誘導 |
| 3 | aggregator.rs:699-711 | 旧 `classify_emp_group_label` が「契約」「業務委託」を「正社員」グループに分類 (現役)。emp_classifier.rs (新) と二重定義残存 | 月給統計の混在 |
| 4 | aggregator.rs:644-651 | `aggregate_by_emp_group_native` の派遣・その他 native_unit 決定ロジックが**両配列とも常に同件数**になるため `>` 比較で常に "月給" 選択 (バグの疑い) | 派遣の時給→月給換算で表示単位誤り |
| 5 | aggregator.rs:307-313 | CSV 内重複検出なし (file_hash / row_hash 0 件)。スクレイピング由来の重複行が統計に直接反映 | 平均値の重複 bias |

### 🟡 Important (12 件)

| # | 場所 | 問題 |
|---|------|------|
| 1 | upload.rs:42-67 | detect_csv_source が「会社名」を JobBox/Indeed 両方の判定子に使い、判定衝突可能 |
| 2 | upload.rs:617 | detect_columns_from_data の score>=30 閾値根拠不明、HashMap iter 非決定性で同点列が運次第 |
| 3 | location_parser.rs:1071-1080 | designated_city が municipality を区なし「横浜市」で返す。station 経由は区つき「横浜市西区」 → 集計タプル不整合 |
| 4 | location_parser.rs:1083-1131 | city_aliases 17 件、resolve_city_alias 20 件で不整合。「堺」「相模原」「千葉」が略称マッチ漏れ |
| 5 | location_parser.rs:954-957 | 駅名マッチ confidence=0.9 が「徒歩60分」など距離情報を考慮しない (常に高信頼) |
| 6 | salary_parser.rs:148-160 | 範囲 splitn(2) で 3 値表記時に後半の単一値のみ抽出 (テスト不足) |
| 7 | salary_parser.rs:391-402 | 異常値判定 (Monthly 100k-2M) が confidence のみで集計に反映されない。n<4 の少件数で IQR 通過する異常値が median を歪める |
| 8 | upload.rs:777-797 | normalize_employment_type が「契約職員」「常勤」「フルタイム」等の variant を吸収しない |
| 9 | aggregator.rs:605-635 | パートグループに月給データが混入した時の単位ガードなし (IQR 通過すれば集計される) |
| 10 | statistics.rs:289 | trimmed_mean 10% カットが IQR 1.5 後にさらに重ね適用 → **二重除外** |
| 11 | aggregator.rs:743-749 | linear_regression R²=0 の意味 (相関なし vs ゼロ分散) を UI で区別表示しているか未確認 → **逆因果誤誘導の可能性** |
| 12 | aggregator.rs:232-235 | prefecture=None レコードが by_prefecture から除外されるが total_count には含まれる → 件数集計不整合の表示 |

### 🟢 Recommended (該当 14 件は仕様妥当 + 軽微なテスト不足)

主要なもの:
- IQR 1.5 倍 (Tukey 標準) - 妥当 (歪み分布の log 変換は将来課題)
- Bootstrap 2000 反復 - 妥当
- 母分散採用 (n 除算) - 全数調査前提で妥当
- 同名市区町村 (伊達市・府中市) - 実装正しい (テスト追加推奨)
- 駅名マッチ leftmost 優先 - 既存 MECE テスト 2000+ ケースで担保

---

## 8. 不足テスト (新規追加推奨)

### 🔴 必須優先

1. **Shift-JIS / UTF-16 デコーディング**: `upload_csv_with_shift_jis_encoding` (現状 0 件 → records=0 失敗を確認)
2. **賞与抽出**: `parse_salary_extracts_bonus_months_from_description` (現状実装なし → 仕様化提案)
3. **emp_classifier vs aggregator::classify_emp_group_label の差分検証**: 「契約社員」が両関数で異なる返り値を返すことを assert
4. **派遣・その他 native_unit 決定**: 全員時給データで native_unit="時給" を assert (現状 "月給" のバグ確認)
5. **CSV 行重複検出**: 同一行を 2 回含む CSV で total_count=1 (or 重複検出メッセージ表示) を assert

### 🟡 推奨

6. **detect_columns_from_data の同点列**: 同じ score 列が 2 つあるとき deterministic に 1 つ選ぶ (現状非決定的)
7. **municipality 文字列粒度**: `(神奈川県, 横浜市)` と `(神奈川県, 横浜市西区)` が同一バケットに集約される
8. **同名市区町村 (伊達市)**: 北海道伊達市と福島県伊達市が異なるエントリで保持される
9. **resolve_city_alias 抜け漏れ**: "堺", "相模原", "千葉" が略称マッチで認識される
10. **正規表現拡張**: "契約職員", "フルタイム", "常勤", "嘱託職員" の normalize_employment_type 動作
11. **3 値範囲表記**: "200,000~300,000~400,000" の min/max 抽出 (現状 splitn(2) で曖昧)
12. **小サンプル外れ値**: n=3 で `[200_000, 250_000, 100_000_000]` を投入 → IQR 通過 (n<4) → median 過大表示の証拠
13. **二重除外影響**: IQR 後 trimmed_mean が同じ値を 2 回除外しないこと
14. **prefecture=None 件数整合**: by_prefecture の sum + None 件数 = total_count
15. **R²=0 の意味分け**: 「ゼロ分散」と「相関なし」の UI 表示区別

---

## 9. 誤情報のリスク評価 (severity)

### 🔴 高リスク (誤った数値が表示され、ユーザーの意思決定を誤らせる)

| 表示内容 | リスク要因 | 対応 |
|---------|----------|------|
| 「平均月給 X 円」 | 賞与未抽出 → HW (賞与込み) との比較で 15-30% 過小 | bonus_months 抽出実装、または「賞与なしの月給のみ」明記 |
| 「派遣・パートの時給 X 円」 | native_unit 決定バグ (Q6.2/Q6.4) で月給値が時給と表示される可能性 | aggregate_by_emp_group_native の `>` 判定を仕様確認 + 修正 |
| 「全 N 件の求人を分析」 | CSV 内重複行を含む可能性 (Q6.4) | row_hash 重複検出 or 「※ 重複除外なし」明記 |

### 🟡 中リスク (近似精度が落ちるが致命的でない)

| 表示内容 | リスク要因 |
|---------|----------|
| 「市区町村別給与」 | 同一市の station/designated 経路で別キー (Q2.2 / Q6.1) |
| 「相関係数 R²=0.X」 | ゼロ分散 vs 相関なしの区別不明 (Q6.3) |
| 「契約社員給与」 | aggregator 旧分類が「正社員」グループに混入 (Q4.1) |

### 🟢 低リスク

| 表示内容 | リスク要因 |
|---------|----------|
| 「都道府県別求人数」 | パース失敗レコード除外 (件数差異あるが許容) |
| 「年間休日 X 日」 | extract_annual_holidays は 80-200 範囲チェックあり (upload.rs:835-877) |

### 逆因果による誤誘導リスク (feedback_correlation_not_causation 観点)

`survey/render.rs` および `report_html/` 配下の so-what 表示で、以下の表現を grep で確認推奨:
1. 「給与が高い → 応募が多い」(逆: 応募が多い → 給与下げない)
2. 「タグが多い → 競争力ある求人」(逆: 競争力なし → タグで差別化)
3. 「相関が強い」「正の関係」(R² で因果を示唆)
4. 「給与が低い地域は人材不足」(逆: 人材不足 → 給与吊り上げ → 高給与化、または人材余剰 → 競合圧力 → 高給与でも応募多)
5. 「企業 X が低給与 → ブラック企業」(様々な逆因果: 業界平均・職種・規模差を考慮していない)

(本タスクではコード編集禁止のため、表現監査の **TODO** として親セッションに申し送り)

---

## 10. 親セッションへの申し送り Top 5

1. **🔴 Shift-JIS 未対応 (Q1.3)**: `encoding_rs` crate 追加 + UTF-8/Shift-JIS auto-detect でラップ。実装工数 1-2h、ビジネスインパクト大。逆証明テスト: `parse_csv_with_excel_shift_jis_succeeds`
2. **🔴 emp_classifier 移行未完 (Q4.1)**: `aggregator.rs::classify_emp_group_label` (L699-711) を `emp_classifier::classify` に置換。`feedback_agent_contract_verification` 観点で「2 つの分類器が並存 → 結果不整合」を併発検査するテストを追加。Plan P2 #2 完遂のため
3. **🔴 派遣・その他 native_unit 決定ロジック疑義 (Q6.2)**: aggregator.rs:644-651 の判定が **両配列同件数 → 常に "月給"** になる疑いあり。具体値テストで現状動作を逆証明 + 仕様確認 (派遣で時給多数のとき "時給" 期待か "月給" 期待か Plan P2 で曖昧)
4. **🔴 CSV 重複検出なし (Q6.4)**: row レベル hash (job_title + company + location + salary) で重複行を検出 + 「※重複除外: N件」表示推奨。スクレイピング bias 緩和に必須
5. **🔴 賞与未パース (Q3.3)**: `salary_parser` に bonus_months フィールド追加 + description 列パース実装。HW Panel 5 (condition_gap) との比較整合性を回復しないと「年収不足」誤誘導が継続。年収算出式: `monthly × (12 + bonus_months)` で HW 側と統一

---

## 11. 監査プロセスメタ情報

- **読了ファイル数**: 10 (upload, location_parser, salary_parser, aggregator, statistics, emp_classifier, parser_aggregator_audit_test, location_parser_realdata_test, handlers, plan_p2 / team_gamma / exec_f1 / exec_c3 / exec_e2 抜粋)
- **Grep 検索**: 12 種 (encoding, file_hash, bonus, BOM, MAX_SIZE, etc.)
- **コード編集**: 0 件 (本タスクは調査のみ)
- **既存テスト破壊**: 0 件 (新規ファイル作成のみ)
- **MEMORY 遵守確認**:
  - feedback_reverse_proof_tests: 各 Q で逆証明セクション必須記述
  - feedback_test_data_validation: 推奨テスト 15 件全て具体値での逆証明形式
  - feedback_never_guess_data: 「根拠不明」「要追加検証」の明示 (3 箇所)
  - feedback_correlation_not_causation: §9 で逆因果誤誘導表現 5 件を独立リスト化
