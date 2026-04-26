# Exec F3 Results: format!→write! バルク変換 + unwrap() 削減

**実施日**: 2026-04-26
**担当**: Agent F3 (Refactoring Expert)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**根拠監査**:
- `docs/audit_2026_04_24/plan_p3_code_health.md` #10, #13
- `docs/audit_2026_04_24/team_delta_codehealth.md` §7.1, §7.3

**作業範囲**: F2 担当ファイル (`survey/report_html.rs`, `analysis/render.rs`, `analysis/fetch.rs`) を **除外** した全ハンドラ。

---

## 1. format! → write! バルク変換 結果

### ファイル別変換数

| ファイル | 変換数 | 備考 |
|---|---|---|
| `src/handlers/insight/render.rs` | **44** | サブタブ4種 + insight report ページ |
| `src/handlers/company/render.rs` | **30** | 会社詳細レンダラ |
| `src/handlers/jobmap/render.rs` | **16** | 求人地図レンダラ |
| `src/handlers/diagnostic.rs` | **13** | 診断レーダー |
| `src/handlers/survey/render.rs` | **11** | survey UIレンダラ |
| `src/handlers/survey/integration.rs` | **11** | survey データ統合 |
| `src/handlers/competitive/render.rs` | **10** | 競合分析レンダラ |
| `src/handlers/jobmap/region.rs` | **5** | 地域カード |
| `src/handlers/region/karte.rs` | **4** | 地域カルテ |
| `src/handlers/jobmap/handlers.rs` | **3** | jobmap ハンドラ |
| `src/handlers/competitive/handlers.rs` | **3** | 競合ハンドラ (1件は手動補完) |
| `src/handlers/recruitment_diag/render.rs` | **2** | 採用診断レンダラ |
| `src/handlers/insight/handlers.rs` | **2** | insight ハンドラ |
| `src/handlers/trend/handlers.rs` | **2** | トレンドハンドラ |
| `src/handlers/analysis/handlers.rs` | **2** | analysis ハンドラ (※F2の `render.rs/fetch.rs` 以外) |
| `src/handlers/trend/render.rs` | **1** | トレンドレンダラ |
| `src/handlers/market.rs` | **1** | 市場ハンドラ |
| **F3 合計** | **160** | |

**F2 担当 (未実施、F3 範囲外)**:
- `src/handlers/analysis/render.rs` 残 82 箇所
- `src/handlers/survey/report_html.rs` 残 75 箇所

### 変換パターン

**Before**:
```rust
html.push_str(&format!(
    "<div class=\"kpi\">{title}</div>",
    title = escape_html(t),
));
```

**After**:
```rust
write!(html,
    "<div class=\"kpi\">{title}</div>",
    title = escape_html(t),
).unwrap();
```

各ファイル冒頭に `use std::fmt::Write as _;` を追加。

### パフォーマンス改善見込み

- **中間 String 確保削減**: 160 箇所 × 推定 50〜200 byte/箇所 = 約 8〜32 KB のヒープ確保削減 / リクエスト
- **`.push_str(&format!(...))`** は `String::write_fmt` 経由の `write!` と等価だが、前者は中間 `String` を生成しドロップする 1 ステップ余分なヒープ往復が発生
- **HTML 生成パスの典型**: insight/render.rs の 44 箇所が 1 リクエストで全実行される場合、累積で μs 〜 数 ms オーダーのスループット改善見込み (実測未実施)
- **HTML 出力バイト不変**: `format_args!` 経由で同じフォーマット展開のため出力差なし

### 安全性

- `write!(String, ...)` は `Result<(), std::fmt::Error>` を返すが `String` への書込は **絶対に失敗しない** (`Write for String` は `Result::Ok` のみ返す)
- `.unwrap()` は `Cargo.toml [lints.clippy] unwrap_used = "allow"` に準拠
- panic リスク追加なし (静的に到達不能)

---

## 2. .unwrap() 削減 結果

### 監査時(2026-04-24)→ 現状(2026-04-26) の差分

監査時の production 経路 unwrap 件数 vs 現状:

| ファイル | 監査時 | 現状(write!変換前) | 現状(変換後) | 備考 |
|---|---|---|---|---|
| `insight/handlers.rs` | 26 | **0** | 2 (write!由来) | F1/F2 で全て削減済み。F3 の write! 変換で 2 追加(String 書込みで安全) |
| `survey/handlers.rs` | 25 | **0** | 0 | F1/F2 で全削減済み |
| `recruitment_diag/handlers.rs` | 23 | 1 (テスト内) | 1 (テスト内) | production 0 |
| `region/karte.rs` | 13 | 1 | **0** (F3 修正) | line 124 の `state.hw_db.as_ref().unwrap()` を graceful 空応答に置換 |
| `survey/report_html.rs` | 13 | (F2 担当範囲) | (F2 担当範囲) | F3 範囲外 |
| `local_sqlite.rs` | 24 | 24 (全テスト内) | 24 (全テスト内) | production 0 |

### F3 が修正した production unwrap

**1. `src/handlers/region/karte.rs:124` の `state.hw_db.as_ref().unwrap().clone()`**

**Before**:
```rust
let db = state.hw_db.as_ref().unwrap().clone();
```

**After**:
```rust
// SAFETY: pref/muni が空でない = lookup_pref_muni が Some を返した = state.hw_db は Some
// それでも graceful な空応答に置換し panic 経路を除去
let db = match state.hw_db.as_ref() {
    Some(db) => db.clone(),
    None => {
        return axum::Json(json!({
            "error": "hw_db unavailable",
            "citycode": citycode,
        }));
    }
};
```

論理的には到達不能 (上流で Some 保証) だが、防御的プログラミングで panic 経路を完全に除去。

### 追加 .unwrap() の安全性

F3 の write! 変換で `.unwrap()` を 160 箇所追加したが、**全て String 書込み** で `std::fmt::Write for String` の実装が `Ok(())` のみ返すため panic 不可能。

```rust
// stdlib (確認済み)
impl Write for String {
    fn write_str(&mut self, s: &str) -> Result {
        self.push_str(s);
        Ok(())
    }
}
```

### Production unwrap 残存 (F3 範囲)

担当ファイル群で F2 領域を除く production 経路の真の `.unwrap()`:

| ファイル | 残数 | 評価 |
|---|---|---|
| `region/karte.rs` | 0 | F3 で 1 → 0 |
| `survey/handlers.rs` | 0 | 既に解消 |
| `insight/handlers.rs` | 0 (write!由来除く) | 既に解消 |
| `recruitment_diag/handlers.rs` | 0 (テスト内 1) | 既に解消 |

F3 担当範囲では production 経路の真の unwrap は **0 箇所**を達成。

### F3 範囲外 (F2 / 別 sprint で対応):
- `analysis/render/mod.rs:2732` `data.last().unwrap()` (空 Vec で panic 可能性) — F2 領域
- `analysis/render/mod.rs:4089/4215/4231/4397` `serde_json::Number::from_f64(v).unwrap()` (NaN/Inf で panic 可能性) — F2 領域
- `auth/session.rs` Mutex lock unwrap × 3 — poisoned 時 panic、慣習的に許容
- `config.rs` ENV_LOCK unwrap × 5 — テストヘルパ、許容
- `db/cache.rs:175` thread join unwrap — テスト
- `company/handlers.rs:259/265` 静的 `.parse::<HeaderValue>().unwrap()` — リテラル parse、絶対失敗しない

---

## 3. clippy warn 解消

未着手。redundant_clone / needless_collect は別 sprint。

---

## 4. 検証結果

### ビルド
```
cd hellowork-deploy && cargo build
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.35s
→ warnings: 4 件 (全て F1/F2 進行中の dead_code、F3 起因なし)
   - fetch_industry_structure (F1 領域)
   - render_survey_report_page (F2 PDF 再構成中)
   - render_comparison_card (F2 PDF 再構成中)
   - render_section_hw_comparison (F2 PDF 再構成中)
```

✅ **lib ビルド: 成功**

### テスト

**F3 開始時のベースライン**: 670 passed, 1 ignored (lib tests)

**F3 完了後**:
- `cargo build` (lib): 成功
- `cargo test --lib`: **コンパイルエラー 1 件** (F1/F2 作業中の既知の不整合)
  ```
  error[E0609]: no field `sample_count` on type `&EmpGroupNativeAgg`
      --> src\handlers\survey\parser_aggregator_audit_test.rs:1040:24
  ```

**原因**: `src/handlers/survey/aggregator.rs::EmpGroupNativeAgg` 構造体は `count: usize` のみ定義。F1/F2 が working tree で `parser_aggregator_audit_test.rs` に新たな assert (`part_group.sample_count > 0`) を追加したが、対応する struct 側のフィールドリネーム/追加が未完了。

**F3 との関係**: 完全に無関係。`survey/parser_aggregator_audit_test.rs` は F3 の write! 変換対象ではない (未編集)。`survey/aggregator.rs` も未編集。本ファイルは F1/F2 の進行中作業領域。

**回避策**: F1/F2 の sample_count rename 完了を待つ。または、`assert!(part_group.count > 0, ...)` への置換 (F1/F2 タスク領域のため F3 では実施しない)。

---

## 5. 親セッション統合チェックリスト

### F3 完了済み
- [x] `format! → write!` バルク変換 160 箇所 (F2 領域除く全ハンドラ)
- [x] 各対象ファイルへの `use std::fmt::Write as _;` 追加 (トップレベル)
- [x] production `.unwrap()` 削減: `region/karte.rs:124` graceful 化
- [x] `cargo build` 成功維持
- [x] HTML 出力バイト不変 (`write!` は `format!` と format_args! 互換)
- [x] 公開 API シグネチャ不変

### F2 へ申し送り (F3 範囲外)
- [ ] `survey/report_html.rs` 残 75 箇所の write! 変換 (PDF 再構成 完了後)
- [ ] `analysis/render.rs` 残 82 箇所 → 既に分割済み `analysis/render/mod.rs` (F2 完了済み？要確認)
- [ ] `survey/parser_aggregator_audit_test.rs:1040` の `sample_count` を `count` に置換 OR aggregator.rs 側に sample_count フィールド追加 (F1/F2 一時的な不整合)
- [ ] `analysis/render/mod.rs:2732, 4089, 4215, 4231, 4397` の production unwrap 削減

### 検証推奨 (F3 完了後にユーザー or 親セッション)
- [ ] F1/F2 完了後に `cargo test --lib` で 670+ tests 維持を再確認
- [ ] 既存の 1 ignored (`bug_marker_*`) は維持
- [ ] HTML 出力バイト一致のスナップショットテスト (任意、`cargo test` 内に既存あれば自動)

### 小バッチ commit 推奨 (revert 容易性)

F3 の変更は 1 PR だが、commit 単位は以下のように分割推奨:
1. `chore(insight): convert html.push_str(format!) to write! (44 places)`
2. `chore(company): convert html.push_str(format!) to write! (30 places)`
3. `chore(jobmap): convert html.push_str(format!) to write! (24 places)` (render.rs 16 + handlers.rs 3 + region.rs 5)
4. `chore(competitive): convert html.push_str(format!) to write! (13 places)` (render.rs 10 + handlers.rs 3)
5. `chore(survey): convert html.push_str(format!) to write! in non-F2 files (22 places)` (render.rs 11 + integration.rs 11)
6. `chore(diagnostic|trend|market|analysis_handlers|recruitment_diag|region_karte): convert html.push_str(format!) to write! (27 places)`
7. `refactor(region/karte): replace state.hw_db.unwrap() with graceful empty response`

---

## 6. 制約遵守確認

| 制約 | 遵守 |
|---|---|
| F2 担当ファイル (`survey/report_html.rs`, `analysis/render.rs`, `analysis/fetch.rs`) を触らない | ✅ 確認済み |
| 既存テスト破壊禁止 | ✅ F3 起因の破壊なし (F1/F2 作業中の sample_count 不整合は別件) |
| ビルド常時パス | ✅ `cargo build` 成功 |
| 公開 API シグネチャ不変 | ✅ pub fn の引数/戻り値型変更なし |
| `feedback_partial_commit_verify.md` (依存チェーン確認) | ✅ `use std::fmt::Write` のスコープ確認、関数内 use を回避 |
| `feedback_test_data_validation.md` (意味保存) | ✅ `format!` と `write!` は format_args! 経由で同一バイト出力 |
| memory `feedback_implement_once.md` (一発で完了) | 🟡 F1/F2 の進行中作業との衝突で sample_count 既知エラーは残置 |

---

## 7. 工数実績

- 計画: 1.0 人日 (Morphllm 利用想定)
- 実績: 約 0.5 人日 (Python スクリプト自作で自動化、Morphllm 不使用)

スクリプトは `find_matching_paren` で Rust トークン (文字列, raw 文字列, char リテラル, 行/ブロックコメント) を尊重しつつ `html.push_str(&format!(...))` を `write!(html, ...).unwrap()` に置換。160 箇所を 5 分以内で変換完了。

---

**完了報告レベル**: 🟡 **基盤完了 (build pass) + 本体実装完了 (160 箇所変換 + 1 unwrap 削減)**

- ✅ コード変換完了
- ✅ `cargo build` 成功
- ❌ `cargo test --lib` は F1/F2 作業中の不整合で 1 件コンパイルエラー (F3 起因ではない)
- ❌ E2E テスト未実施 (F3 スコープ外)

**次ステップ**: F1/F2 の sample_count rename 完了 → `cargo test --lib` 再実行 → 670+ passed 確認 → commit 7 段階分割。
