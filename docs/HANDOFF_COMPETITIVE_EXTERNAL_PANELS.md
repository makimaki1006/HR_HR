# 引き継ぎ: 求人検索タブ 外部統計ドリルダウンパネル (10 ソース MECE)

**作成日**: 2026-06-03
**担当**: Refactoring/Implementation agent (Bash 権限なし)
**親エージェントへの依頼**: build/test/commit/push/PR 作成

---

## 完了した実装内容

### 1. 新規ファイル
- `src/handlers/competitive/external.rs` (約 1180 行)
  - 10 個の axum handler (`ext_min_wage` 〜 `ext_social_life`)
  - 共通ヘルパー: `pref_name_to_code` / `scope_label` / `query_external` /
    `wrap_panel` / `row_f64,i64,string` / `row_string_escaped` / `fmt_f64,i64`
  - mod tests: 13 件のユニットテスト (ヘルパー全部 + 中立表現検証 +
    ドメイン不変条件 + XSS escape)

### 2. 編集ファイル
| ファイル | 編集内容 | 追加行数 (概算) |
|----------|----------|----------------|
| `src/handlers/competitive/mod.rs` | `mod external;` 追加 + `pub use` 10 関数 | +8 |
| `src/lib.rs` | route 10 個追加 (`/api/competitive/external/*`) | +44 |
| `templates/tabs/competitive.html` | `<details>` アコーディオン 10 個 + JS (`loadExternalPanel`/`reloadAllExternalPanels`) | +180 |
| `src/handlers/competitive/tests.rs` | integration 用テスト 4 件追加 (export確認 / endpoint 数 / template marker / 中立性) | +95 |

### 3. 追加した API endpoint (10 個)
すべて `GET /api/competitive/external/{source}?prefecture={pref}`:

| source | テーブル | 形式 |
|--------|---------|------|
| `min_wage` | `v2_external_minimum_wage` | 順位表 + 全国平均比 |
| `job_ratio` | `v2_external_job_openings_ratio` | 年度推移表 |
| `labor_force` | `v2_external_labor_force` | KPI 集計 (失業率/参加率) |
| `turnover` | `v2_external_turnover` | 年度推移表 (入職/離職/差分) |
| `education` | `v2_external_education` | 構成比表 (男女別 + 比率) |
| `industry_employees` | `v2_external_industry_structure` | 上位 15 構成表 |
| `household_spending` | `v2_external_household_spending` | カテゴリ別月額表 |
| `daytime_population` | `v2_external_daytime_population` | KPI + 流入/流出 |
| `households` | `v2_external_households` | KPI (単身率/高齢単身率) |
| `social_life` | `v2_external_social_life` | カテゴリ × 参加率表 |

### 4. テスト件数
- `external.rs` 内 mod tests: **13 件** (XSS escape 追加で +1)
- `tests.rs` 統合用: **4 件** 追加
- **合計 17 件** (要件 15-20 件をクリア)

---

## 親エージェントが実行すべきコマンド (Bash)

```pwsh
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# 1. ブランチ作成
git checkout -b feature/competitive-external-panels

# 2. build
cargo build --release 2>&1 | tail -3

# 3. test (lib のみ、競合せず軽量)
cargo test --release --lib 2>&1 | tail -3
# 期待: 1643 + 17 = 1660 程度 PASS

# 4. 外部統計関連だけ抜き出し検証
cargo test --release --lib --package hellowork_dashboard competitive::external 2>&1 | tail -20
cargo test --release --lib --package hellowork_dashboard competitive::tests::test_external 2>&1 | tail -20

# 5. fmt
cargo fmt --all
git diff --stat  # fmt で変更があれば確認

# 6. commit
git add src/handlers/competitive/external.rs `
        src/handlers/competitive/mod.rs `
        src/handlers/competitive/tests.rs `
        src/lib.rs `
        templates/tabs/competitive.html `
        docs/HANDOFF_COMPETITIVE_EXTERNAL_PANELS.md
git status   # M/?? 一覧で意図と一致するか確認

git commit -m @"
feat(competitive): external data drilldown panels (10 sources MECE)

求人検索タブに HW 以外の公的統計を都道府県粒度で個別表示するアコーディオン
セクションを追加。10 endpoint (最低賃金/求人倍率/失業率/離職率/学歴/産業就業者/
家計支出/昼夜間人口/世帯/社会生活) を <details> 遅延ロードで提供。

- DISPLAY_SPEC §2 遵守 (人数生表示を最小限に、率・順位・推移優先)
- 中立表現徹底 (劣位/集中/縮小 評価語禁止、流入超過/流出超過/均衡)
- silent fallback 禁止 (no_data_html で明示メッセージ)
- XSS 防御: row_string_escaped + wrap_panel での title/scope/source/note escape
- ドメイン不変条件: 失業率/参加率 0〜100% 範囲外を amber 警告表示
- Turso 優先 + ローカル fallback (query_external)
- 既存 fetch.rs/render.rs/handlers.rs 無改変 (疎結合)

Tests: 17 件追加 (external.rs mod tests 13 + tests.rs integration 4)
"@

# 7. push & PR
git push -u origin feature/competitive-external-panels

gh pr create --base main --head feature/competitive-external-panels `
  --title "feat(competitive): external data drilldown panels (10 sources MECE)" `
  --body @"
## Summary
- 求人検索タブに HW 以外の公的統計 10 ソース (賃金/求人倍率/失業/離職/教育/産業/家計/人流/世帯/社会生活) を都道府県粒度で個別表示
- アコーディオン <details> + HTMX 遅延ロード、都道府県セレクタ連動
- 既存求人検索機能と疎結合 (新規 ``external.rs`` のみ、fetch/render/handlers 無改変)

## Test plan
- [ ] ``cargo build --release`` PASS
- [ ] ``cargo test --release --lib`` 1660+ PASS (既存 1643 + 新規 17)
- [ ] ローカル起動で /tab/competitive を開き、外部統計セクションが表示されること
- [ ] 都道府県セレクタを変えると ``data-loaded`` が剥がれ、開閉で再取得されること
- [ ] ネットワーク障害シミュレーション (DevTools) で「取得に失敗しました」表示
- [ ] アコーディオン未展開時はリクエストが飛ばないこと
- [ ] DISPLAY_SPEC §2: 人数生表示が比率・順位より目立たないこと
- [ ] 中立表現: 各パネル title/note に「劣位/集中/縮小」が無いこと

## Notes
- ``v2_external_industry_structure`` は ``prefecture_code`` 主体のため
  ``pref_name_to_code`` で 47 県の名前→コード変換 (ゼロパディング 2 桁)。
- ``v2_external_minimum_wage`` は時系列を持たないため、順位 + 全国平均比で
  「相対位置づけ」を可視化。
- 失業率/参加率は ``(0.0..=100.0)`` ドメイン不変条件チェック (MEMORY:
  feedback_reverse_proof_tests に従う、380% 流出事故の再発防止)。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
"@
```

---

## 重要な設計判断

### A. 既存パターンに従った点
1. **Turso 優先 + ローカルフォールバック**: `analysis::fetch::query_turso_or_local`
   と同等の動作を `query_external` として external.rs に内製
   (private API への結合回避)。
2. **EXTERNAL_CLEAN_FILTER 相当**: `prefecture IS NOT NULL AND ... AND
   municipality <> '市区町村'` を inline。CSV ヘッダー混入レコード防御
   (MEMORY: feedback_silent_fallback_audit)。
3. **HTML escape**: `row_string_escaped` を介して SQL 由来の文字列を
   テンプレ埋込前に escape。`wrap_panel` の title/scope/source/note も escape。
4. **HTMX 連携**: 既存 `onCompPrefChange` 内の `DOMParser` + `adoptNode` パターンを
   踏襲 (セキュリティフックの innerHTML 禁止に従う)。

### B. 既存パターンから外した点
- **`hx-get` 属性ではなく fetch + DOMParser** を採用。理由は
  `<details>` の open 状態と data-loaded 管理を JS で行う必要があり、
  HTMX の `hx-trigger="load"` だと重複 fetch が発生するため。
- **テーブル名 vs コード**: ユーザ指示の "v2_external_industry_employees" は
  実テーブル名 `v2_external_industry_structure` に解釈 (employees_total カラム
  からの就業者構成として整合)。コードコメントで明示。

### C. レポート側との重複回避
レポート (Section 03/04/06/07) では同じテーブルを集計済み数値として表現
していますが、求人検索タブでは「絞り込み中の都道府県でのみ詳細を見る」用途で、
レポートをまたぐ往復を減らす狙い。レポート HTML へのリンクは追加しません
(既存タブの設計外)。

---

## トラブルシュート

### build エラーが出た場合の典型原因
1. **`row_string` を直接埋込んでいる箇所が残っている**:
   `grep -n "row_string(row," src/handlers/competitive/external.rs` で
   `row_string_escaped` 以外の HTML 埋込が残っていないか確認 (170 行目の
   関数定義は無視)。
2. **`super::external::ext_*` の参照ミス**: `tests.rs` で `super::external::*` を
   参照しているが、`mod.rs` で `mod external;` 宣言があるため可視性 OK。
3. **`escape_html` の二重 import 警告**: `use super::utils::escape_html;` が
   external.rs にある。tests.rs にもあるが衝突しないことを確認済み。

### test 失敗の典型原因
1. `test_template_contains_external_section_marker`: テンプレに
   `comp-external-section` ID と 10 個の `data-source="..."` が必要。
2. `test_external_endpoint_count_is_ten`: endpoint 名は重複/抜けがないこと。
3. `test_pref_name_to_code_hokkaido_padding`: 北海道 → "01" ゼロパディング。

---

## 後続作業 (本 PR 範囲外)

- 各 panel に echarts チャート (SVG 簡易表ではなく) を追加
- 月次/年次自動更新ジョブの整備
- HW 求人 (postings) との JOIN 表示 (例: 最低賃金未満求人ハイライト)
