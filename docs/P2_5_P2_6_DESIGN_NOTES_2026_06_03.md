# P2-5 / P2-6 設計案 + 実装引き継ぎ (2026-06-03)

## 背景

- 設計メモ `salary_cluster_analysis_design.md` 未受領
- 2026-06-03 ユーザー指示「pending 全件やり切れ」を受け、subagent (Z) で実装試行 → Bash/Edit 拒否で完遂不能
- subagent Z 報告 (a52d0d1daf187e562) で具体的な実装設計を取得済 → 本 docs に記録
- 既存 PR #14 (P0-9/10/P1-7 anchor id MVP) は main 反映済 (b9dbf3a)
- 残作業 (P2-5/P2-6 実装本体) を本 docs で詳細化し、次セッション・別 AI への引き継ぎ材料とする

## P2-5: 給与構造補助フラグ (業務委託 / 歩合 / 管理職 / 夜勤) 抽出

### 既存実装
- 完全未実装

### 実装ファイル (新規)

`src/handlers/survey/report_html/navy_report/salary_aux_flags.rs` (新規):

```rust
//! P2-5 (MVP, 2026-06-XX): 給与構造補助フラグ抽出
//! 設計メモ未受領のため keyword list は hard-coded、優先順位明示。
//! silent fallback 監査: 未マッチは fallthrough せず明示的に「なし」分類。

use super::super::super::upload::SurveyRecord;

#[derive(Debug, Clone, PartialEq)]
pub struct AuxFlagCount {
    pub label: &'static str,
    pub count: usize,
    pub share_pct: f64,
    pub median_salary_yen: i64,
}

// 優先順位明示 (MECE ではないので重複カウント): 業務委託 > 歩合 > 管理職 > 夜勤 > なし
const KW_GYOMU: &[&str] = &["業務委託", "請負", "フリーランス"];
const KW_BUAI:  &[&str] = &["歩合", "インセンティブ", "コミッション", "成果報酬"];
const KW_KANRI: &[&str] = &["管理職", "マネージャー", "課長", "部長"];
const KW_YAKIN: &[&str] = &["夜勤", "夜間", "シフト 22:", "深夜"];

pub fn extract_aux_flag_summary(records: &[SurveyRecord]) -> Vec<AuxFlagCount> {
    // 1. records を走査
    // 2. 各 record の (title, employment_type, description) を結合
    // 3. KW_GYOMU / KW_BUAI / KW_KANRI / KW_YAKIN いずれの keyword も含まないなら「なし」
    // 4. 重複カウント許容 (1 record が複数 flag を持つ可能性)
    // 5. share_pct = count / records.len() * 100
    // 6. median_salary = グループ内 salary_min の中央値
    todo!()
}
```

### aggregator.rs への field 追加

```rust
pub struct SurveyAggregation {
    // ... 既存 field ...
    pub by_aux_flag: Vec<AuxFlagCount>,  // NEW (P2-5, 2026-06-XX)
}
```

`aggregate_records_core` 内で `by_aux_flag = extract_aux_flag_summary(records);` を追加。

### 表示位置 (section_03_salary.rs)

`render_navy_section_03_salary` 内、扶養範囲到達時給テーブル (`build_navy_fuyou_table`) 直後、SO WHAT 直前:

```rust
// 表 3-Y 給与構造補助フラグ別 件数 (P2-5)
if !agg.by_aux_flag.is_empty() {
    html.push_str("<div class=\"block-title block-title-spaced\">表 3-Y &nbsp;給与構造補助フラグ別 件数 (補助分類)</div>\n");
    html.push_str(&build_navy_aux_flag_table(&agg.by_aux_flag));
}
```

```rust
fn build_navy_aux_flag_table(flags: &[AuxFlagCount]) -> String {
    let mut s = String::from(
        "<table class=\"table-navy\">\n<thead><tr>\
         <th>フラグ</th><th class=\"num\">求人件数</th>\
         <th class=\"num\">構成比</th><th class=\"num\">給与中央値 (万円)</th>\
         </tr></thead>\n<tbody>\n"
    );
    for f in flags {
        s.push_str(&format!(
            "<tr><td><strong>{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{:.1}%</td>\
             <td class=\"num bold\">{:.1}</td></tr>\n",
            f.label,
            f.count,
            f.share_pct,
            f.median_salary_yen as f64 / 10000.0
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">補助フラグは仕事内容/雇用形態/タイトルの keyword 抽出で判定。\
                同一求人が複数フラグに該当する場合は重複カウント。\
                ※ MVP 実装。設計メモ受領後に正規化予定。</p>\n");
    s
}
```

### test (新規 `salary_aux_flags_tests.rs` または同 section_03 内 mod tests)

- `aux_flag_gyomu_matches_request_keyword` (業務委託 keyword 検出)
- `aux_flag_kanri_matches_manager_keyword` (管理職判定)
- `aux_flag_yakin_matches_late_shift` (夜勤判定)
- `aux_flag_combined_count_can_exceed_total` (重複カウント許容)
- `aux_flag_none_when_no_match` (silent fallback 防御)

## P2-6: 業界・職種推定スコアリング (タイトル / タグ / 仕事内容 根拠別)

### 既存実装
- タグ + 会社名の 2 信号源、合算スコアのみ (industry_mismatch.rs)

### 実装ファイル (既存改修)

`src/handlers/survey/report_html/industry_mismatch.rs` (拡張):

```rust
#[derive(Debug, Clone, Default)]
pub struct IndustryScoreBreakdown {
    pub tag_score: f64,         // weight 0.3
    pub company_score: f64,     // weight 0.2
    pub title_score: f64,       // weight 0.3 (NEW)
    pub description_score: f64, // weight 0.2 (NEW)
    pub combined: f64,
}

pub fn compute_industry_score(
    tags: &[(&str, i64)],
    company_name: &str,
    title: &str,
    description: &str,
    industry_keyword_db: &HashMap<String, IndustryKeywords>,
) -> Vec<(String, IndustryScoreBreakdown)> {
    // 各業界について 4 信号源スコアを計算
    // combined = 0.3*tag + 0.2*company + 0.3*title + 0.2*description
    todo!()
}
```

### 表示位置 (section_05_companies.rs)

注目企業表 (`build_navy_notable_companies_block`) 各行に `<details>` を追加:

```html
<td>
  <details>
    <summary>根拠</summary>
    <small>
      T=0.5 (タグ: IT エンジニア) /
      C=0.3 (会社名: 株式会社○○エンジニアリング) /
      Ti=0.4 (タイトル: システム開発) /
      D=0.2 (仕事内容: プログラミング業務)
    </small>
  </details>
</td>
```

### test

- `industry_score_combined_equals_weighted_sum`: 0.3*tag + 0.2*company + 0.3*title + 0.2*description
- `industry_score_title_weight_0_3` (title 重み逆証明)
- `industry_score_zero_when_no_signal`
- `industry_score_clamps_at_one`

## 実装コスト見積

| タスク | 影響範囲 | 工数 (人時) |
|--------|---------|------------|
| P2-5 | aggregator + salary_aux_flags.rs (新規) + section_03_salary.rs + test | 4-6h |
| P2-6 | industry_mismatch.rs + section_05_companies.rs + test | 3-5h |
| 合計 | 4 ファイル変更 + 1 ファイル新規 | **7-11h** |

## 想定表示例

### 表 3-Y (P2-5)

| フラグ | 件数 | 構成比 | 給与中央値 (万円) |
|--------|-----:|------:|---------------:|
| 業務委託 | 12 | 3.4% | 28.0 |
| 歩合 | 8 | 2.3% | 32.5 |
| 管理職 | 15 | 4.3% | 41.0 |
| 夜勤 | 67 | 19.1% | 26.5 |
| なし | 248 | 70.9% | 24.0 |

### Section 05 注目企業 根拠 (P2-6)

> 株式会社○○エンジニアリング — 業界推定: IT (0.78) [根拠 ▼]
> T=0.6 (タグ: ITエンジニア) / C=0.4 (会社名: エンジニアリング) / Ti=0.5 (タイトル: システム開発) / D=0.2 (仕事内容: プログラミング)

## 引き継ぎ

本 docs を元に次セッション・別 AI で本実装を進めること。設計メモ `salary_cluster_analysis_design.md` が repo commit され次第、本 docs の MVP 算式を正規版に置換する。

## 参照

- PR #14 (P0-9/10/P1-7 anchor id): https://github.com/makimaki1006/HR_HR/pull/14
- subagent Z 報告: tasks/a52d0d1daf187e562.output (詳細設計案)
- Parallel B 報告: lp_samples/docs/PENDING_REQUIREMENTS_2026_05_30.md (要件整理)
