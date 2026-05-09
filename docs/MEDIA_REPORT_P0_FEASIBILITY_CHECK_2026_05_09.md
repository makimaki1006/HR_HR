# MEDIA REPORT P0 候補 実現可否検証

**日付**: 2026-05-09
**目的**: `OPEN_DATA_UTILIZATION_TOTAL_AUDIT_2026_05_09.md` で出した旧 P0 と、ユーザーから提示された新 P0 候補について、実装着手前に **データ粒度・fetch・PDF 表示** を read-only で検証し、P0 を最終確定する。
**性質**: read-only。コード変更ゼロ・DB 書込ゼロ・Turso READ ゼロ (Local DB と CSV のみ)。
**指示書遵守**: 検証結果をもって P0 を確定し、本書の承認後に 1 件ずつ実装する。並列実装はしない。

---

## 0. 結論サマリ (3 行)

1. **新 P0-1 (職業×地域×性別×年齢) は完全に既存 DB で実現可能** — `municipality_occupation_population` 729,949 行に gender × age_class × occupation × municipality が全部入っている。実装は表示崩壊修正のみ。
2. **新 P0-2 (CSV求人数 × 地域母集団 4象限) は新規実装ではなく既存ランキング再構成** — `recruiting_scores` 20,845 行に必要な指標 (thickness / 求人数 / priority_score) が全部ある。象限図 UI を作るだけ。**P0 ではなく P1 に降格妥当**。
3. **新 P0-3 (地域×産業×性別) は データ粒度あり、Local 未投入** — Turso スキーマと `fetch_industry_structure.py` 完成。Local 投入 + render 新設の 2 段階。**ユーザーの「DB あり」主張は正しい**。
4. **「地域×産業×性別×年齢」は既存資産では不可能** — 経済センサス R3 (0003449718) は年齢なし、`estat_15_1_merged.csv` は職業軸であって産業軸ではない。新 statsDataId 取得が必要。**P0 から除外**。

---

## 1. 実証データ

### A. `v2_external_industry_structure` 検証

| 項目 | 結果 | 根拠 |
|---|---|---|
| Local DB に投入されているか | ❌ **Local テーブル不在** | `sqlite_master` 検索で `industry_structure` 関連は `v2_cross_industry_competition` のみ |
| `fetch_industry_structure.py` の出力カラム | ✅ **employees_male / employees_female あり** | `scripts/fetch_industry_structure.py:11-13, 102-110` |
| 出力カラム全体 | `prefecture_code, city_code, city_name, industry_code, industry_name, establishments, employees_total, employees_male, employees_female` | 同上 |
| データソース | e-Stat 経済センサス R3 (statsDataId=0003449718) | `fetch_industry_structure.py:5` |
| Turso スキーマ | ✅ **CREATE TABLE 定義あり (male/female 列含む)** | `scripts/upload_new_external_to_turso.py:106-119` |
| 粒度 | `city_code (5桁) × industry_code (A〜S, AS) × 性別 (M/F)` | スキーマより |
| 年齢軸 | ❌ なし | 経済センサスは事業所単位で年齢非公開 |

→ **ユーザーの主張「employees_male / employees_female がある」は事実**。Local 未投入だが Turso 経路は完成済。

### B. `estat_15_1_merged.csv` 検証

| 項目 | 結果 | 根拠 |
|---|---|---|
| ファイル存在 | ✅ 86 MB / 716,959 行 | `data/generated/estat_15_1_merged.csv` (mtime 2026-05-05) |
| ヘッダ | `municipality_code, prefecture, municipality_name, gender, age_class, occupation_code, occupation_name, population, source_name, source_year, fetched_at` | `head -1` |
| サンプル 1 行 | `01100, 北海道, 札幌市, male, 15-19, A, 管理的職業従事者, 1, census_15_1, 2020, ...` | `head -2` |
| データソース | e-Stat 国勢調査 R2 表 15-1 (statsDataId=**0003454508**) | `fetch_estat_15_1.py:5, 50` (ユーザー記憶 0003454503 は 1 桁違い) |
| 粒度 | `municipality_code × gender × age_class × occupation_code × population` | 上記ヘッダ |
| 既に Local DB 投入済か | ✅ `municipality_occupation_population` 729,949 行と一致 | Agent B で確認済 |

→ **これは「職業 × 性別 × 年齢」のデータであり、「産業 × 性別 × 年齢」ではない**。ユーザーの「fetch_estat_15_1.py 系で産業×年齢×性別が取れる」は **誤り** (職業軸である)。
→ ただしこのデータで「**地域×職業×性別×年齢**」は完全に作れる。

### C. `municipality_recruiting_scores` 検証

| 項目 | 結果 |
|---|---|
| 行数 | 20,845 |
| カラム数 | 23 |
| 主要カラム | `municipality_code, prefecture, municipality_name, basis, occupation_code, occupation_name, distribution_priority_score, target_thickness_index, commute_access_score, competition_score, salary_living_score, rank_in_occupation, rank_percentile, distribution_priority` (S/A/B/C/D), 3 シナリオスコア |
| `distribution_priority` 分布 | S=407, A=627, B=2,090, C=7,293, D=10,428 |
| 新宿区 13104 サンプル (resident basis) | `priority_score=84.03, thickness=200.0, commute=0.792, competition=4.59, salary_living=179.81` (5 職業すべて同値、職業ごとの差分は rank_percentile のみ) |
| 既存 render 関数 | `render_mi_distribution_ranking` (line 1544) / `render_mi_parent_ward_ranking` (line 1203) |
| 4 象限 / quadrant の既存実装 | ❌ **ゼロ** (grep 0 hit) |
| 表現形式 | parent_code でグループ化したランキング表 (table) |

→ **データはフル揃っている**。象限図 UI を新設すれば「CSV 求人数 × 地域母集団」の 4 象限化は可能。新規実装は **データ取得ではなく UI 表現の追加** だけ。

### D. PDF 実物 (再確認)

`out/round3_cprime_pdf_review/mi_via_action_bar.pdf` (2026-05-09 17:38、26 ページ):

| 項目 | 状態 |
|---|---|
| P19-20: 配信地域ランキング | ✅ 表示済 (4 自治体)、ただし「S/A 該当なし」表示 (新宿区は priority_score=84 で本来 S 相当だが priority='C') |
| P22-24: 職業×地域マトリクス | △ 「北海道 伊達市」が 100 行近く列挙、新宿区不在 |
| 産業構成 (P8-9) | ✅ MI で表示済 (`v2_external_industry_structure` 経由、ただし Local 未投入で Turso fetch 経路) |
| 産業 × 性別 | ❌ 不在 |
| 職業 × 性別 × 年齢 (1 ページ集約) | ❌ 不在 |

---

## 2. 出力表 (判定)

| 候補 P0 | データ粒度 | DB/CSV 存在 | fetch 存在 | PDF 表示 | 判定 |
|---|---|---|---|---|---|
| **地域×職業×性別×年齢** | muni × 職業大分類 × 性別(M/F) × 年齢(5歳刻み) | ✅ Local `municipality_occupation_population` 729,949 行 | ✅ `fetch_estat_15_1.py` 完了 | △ MI で対象外自治体羅列の表示崩壊 | **採択 (P0)** |
| **地域×産業×性別** | city × 産業(A〜S) × 性別(M/F) | 🟡 Local 未投入 / Turso スキーマあり / 完成 fetch あり | ✅ `fetch_industry_structure.py` 完了 | ❌ MI は性別なし産業 Top 10 のみ | **採択 (P0)** ※ Local 投入か Turso fetch 経路かは別途決定 |
| 地域×産業×性別×年齢 | ❌ 経済センサス R3 (0003449718) に年齢なし / estat_15_1 (0003454508) は職業軸 | ❌ 既存資産で不可 | ❌ 該当 fetch なし | ❌ | **保留 / 除外** (新 statsDataId 取得要) |
| CSV求人数×地域母集団 (4象限) | muni × 求人数 + thickness + 4 スコア | ✅ `municipality_recruiting_scores` 20,845 行 | ✅ 集計済 | △ ランキング表で表現済、4 象限図はなし | **P1 降格** (新規データ取得不要、UI 表現変更のみ) |

### 判定ルール対応

| ルール | 該当候補 | 判定 |
|---|---|---|
| DB/CSV に粒度なし → P0 にしない | 地域×産業×性別×年齢 | 除外 |
| 粒度はあるが未 fetch → P1 または別タスク | (該当なし、産業×性別は fetch 完成済) | — |
| fetch 済みで PDF 未表示 → P0 候補 | 地域×産業×性別 | **P0** |
| PDF 表示済みだが読めない → P0 候補 | 地域×職業×性別×年齢 | **P0** |
| 既に表示済みで表現変更だけ → P1 | CSV 求人数×地域母集団 (象限化) | **P1** |

---

## 3. 確定 P0 (2 件、順次実装)

### P0-1: 地域 × 職業分類 × 性別 × 年齢 を MI PDF に 1〜2 ページで読める形に

| 項目 | 内容 |
|---|---|
| データ | `municipality_occupation_population` (729,949 行 / Local 投入済) |
| 既存実装 | `render_mi_talent_supply` / `render_mi_occupation_cells` (`market_intelligence.rs:242` 周辺) |
| 現状の問題 | 対象自治体フィルタが効いておらず「北海道 伊達市」が大量列挙される (PDF P22-24 全 26 ページの 12% を浪費)。新宿区の数字が読めない |
| 必要作業 | (a) フィルタを `target_municipalities` 限定にする (b) 表形式を「対象地 × 上位 N 職業 × 性別 × 年齢層 (3 区分: 〜29/30〜49/50〜)」のサマリ表に圧縮 (c) 「採用示唆」列を 1 行追加 |
| 完了条件 | 対象地の職業×年齢×性別が **1〜2 ページ** で読める / 誤地表示が **5 件以下** / 採用示唆 (例: 「女性×40 代以上が厚い」) が機械的に出る |
| 実装範囲 | `market_intelligence.rs` のフィルタ + 整形のみ。新規 fetch / 新規 DTO 不要 |
| リスク | 中 (UI 表現の再設計、既存テストの修正) |

### P0-2: 地域 × 産業 × 性別 を MI PDF に出す

| 項目 | 内容 |
|---|---|
| データ | `v2_external_industry_structure` (city × 産業 × employees_male/female) |
| 既存実装 | 性別なしの産業構成 Top 10 のみ (`region.rs:269 render_section_industry_structure`) |
| 現状の問題 | Local 未投入。fetch スクリプト・Turso スキーマ・upload script は完成済だが、ローカル開発時に再現できない |
| 必要作業 | **2 段階** (a) Local 投入 (ユーザー手動 / `SURVEY_..._PHASE3_TABLE_INGEST.md` の手順) (b) `fetch_industry_structure_with_gender` 追加 + `render_section_industry_gender` 新設 + MI variant に接続 |
| 完了条件 | 対象地の産業 Top 10 が **男女比付き** で表示 / 採用示唆 (例: 医療・福祉は女性 72%) が出る |
| 実装範囲 | DB 投入 1 + fetch 1 関数 + DTO 1 フィールド + render 1 関数 + mod.rs 接続 |
| リスク | 高 (Local DB 投入を伴う。ユーザー手動オペレーション必要) |

---

## 4. P1 (P0 完了後)

| ID | タスク | 根拠 |
|---|-------|------|
| P1-1 | CSV 求人数 × 地域母集団 4 象限図 | データ揃い済 (`recruiting_scores`)、UI 表現変更のみ |
| P1-2 | 最低賃金 DB 接続 (`helpers.rs:936-958` ハードコード解除) | 給与妥当性の運用リスク |
| P1-3 | 産業構造空配列バグ修正 (`insight/fetch.rs:179`) | Full/Public 経路の silent 空白化 |
| P1-4 | 昼間人口 (`ext_daytime_pop`) を survey PDF render に連結 | survey PDF 参照ゼロ |
| P1-5 | 生活コスト粒度改善 (家計支出列の追加投入) | 「都道府県値流用」注記の解消 |
| P1-6 | priority='C' なのに priority_score=84 の整合性問題 | recruiting_scores で `S/A 該当なし` 表示の真因の可能性あり、要追加調査 |

---

## 5. 除外 / 保留

| 項目 | 理由 |
|---|---|
| 地域 × 産業 × 性別 × 年齢 | 既存資産に粒度なし。新 statsDataId 取得 + DB 投入 + render 新設は本ラウンドの範囲外 |
| 業界別給与 / 職種別給与 (Round 3-A〜3-C') | CSV に該当列なし。既決定方針を維持 |
| 医療介護需要・地価・自動車保有・気候 | 採用示唆クロスへの寄与度が中以下、本ラウンドの範囲外 |

---

## 6. 着手順 (ユーザー承認後)

1. **P0-1 のみ着手** (Local DB に必要データが既にあるため即実装可能)
2. P0-1 完了 (PDF 実物で「対象地が読める」「誤地 5 件以下」を満たすこと) を確認後、P0-2 着手判断
3. P0-2 は Local 投入が前提。ユーザーの DB 投入オペレーション完了後に着手 (Claude は DB 書込禁止)
4. P0 完了まで P1 は触らない

---

## 7. 監査メタデータ

- 検証時刻: 2026-05-09
- 検証範囲: ローカル DB schema (sqlite3 PRAGMA), CSV ヘッダ + 5 行 sample, fetch script grep, render 関数定義
- DB 書込: ゼロ
- コード変更: ゼロ
- Turso READ: ゼロ
- 関連 docs: `OPEN_DATA_UTILIZATION_TOTAL_AUDIT_2026_05_09.md` (本書の前段)
- 次の意思決定: 本確定 P0 (2 件) の承認 → P0-1 から実装着手
