# Dead Route 確認手順書

**作成日**: 2026-04-26
**作成者**: Refactoring Expert (Agent E3)
**目的**: `plan_p3_code_health.md #5` で発見された「ナビ非搭載だが実は外部 API として現役の可能性があるルート」の生死判定手順を整備し、誤削除によるロールバックリスクを回避する
**配置先 (本来)**: `docs/dead_route_audit.md` (sandbox 制約により audit ディレクトリに暫定配置)
**🔴 重要**: 本ドキュメントは確認手順 **のみ** を整備する。**削除実行は禁止**。

---

## 1. 対象ルート (2026-04-26 時点)

`src/lib.rs` で定義されているが `templates/dashboard_inline.html:70-89` のナビには **存在しない** 6 ルート + 4 API:

### 1.1 タブ系 (UI 経路なし)

| ルート | ハンドラ | テンプレート | 削除候補 |
|---|---|---|---|
| `/tab/overview` | `handlers::overview::tab_overview` | `templates/tabs/overview.html` | 要確認 |
| `/tab/balance` | `handlers::balance::tab_balance` | `templates/tabs/balance.html` | 要確認 |
| `/tab/workstyle` | `handlers::workstyle::tab_workstyle` | `templates/tabs/workstyle.html` | 要確認 |
| `/tab/demographics` | `handlers::demographics::tab_demographics` | `templates/tabs/demographics.html` | 要確認 |
| `/tab/trend` | `handlers::trend::tab_trend` | `trend/render.rs` | 要確認 |
| `/tab/insight` | `handlers::insight::tab_insight` | `insight/render.rs` | 要確認 |

### 1.2 Insight API (外部公開の可能性)

| ルート | 公開証拠 | 判定 |
|---|---|---|
| `/api/insight/report` | `README.md:21-22`, `docs/openapi.yaml:193`, `e2e_api_excel.py:184-188` | **外部 API として現役の可能性大 — 削除前提なし** |
| `/api/insight/report/xlsx` | `README.md:22`, `docs/openapi.yaml:200`, `e2e_api_excel.py:259-264` | 同上 |
| `/api/insight/widget/*` | OpenAPI 公開 | 同上 |
| `/api/insight/subtab/*` | OpenAPI 公開 | 同上 |

`docs/contract_audit_2026_04_23.md:30` には「frontend consumer なし」とあるが OpenAPI に載っている = MCP/AI 連携用と判断される。**削除前にアクセスログ確認必須**。

---

## 2. Stage 1: 外部利用ログ確認 (削除前の必須前提)

### 2.1 Render ログ (本番)

Render dashboard で過去 7 日のアクセスログを確認:

1. Render dashboard にログイン
2. 該当 Web Service → "Logs" タブ
3. 検索パターン:
   ```
   /api/insight/report
   /api/insight/widget
   /api/insight/subtab
   /tab/overview
   /tab/balance
   /tab/workstyle
   /tab/demographics
   /tab/trend
   /tab/insight
   ```
4. 結果を CSV にエクスポート (アクセス時刻 / IP / User-Agent / status)

### 2.2 nginx ログ (もしあれば)

```bash
grep -E '(/api/insight/(report|widget|subtab)|/tab/(overview|balance|workstyle|demographics|trend|insight))' \
    /var/log/nginx/access.log \
  | awk '{print $7}' | sort | uniq -c | sort -rn
```

### 2.3 判定基準

Stage 1 の結果を以下の 3 区分で判定:

| 区分 | 条件 | 次アクション |
|---|---|---|
| **A. UI 復活** | ユーザー由来の高頻度アクセスあり (例: 1 日 100 件超) | ナビに 1 行追加 (削除しない) |
| **B. 外部 API のみ生存** | `/api/insight/*` に MCP/AI 経由の定常アクセスあり、`/tab/*` は 0 | `/tab/*` のみ削除、`/api/insight/*` 維持 |
| **C. 完全 dead** | 7 日間アクセス 0 件 | ハンドラ・ルート・テンプレート削除可 |

---

## 3. Stage 2: テンプレート遺物確認

V1 求職者ダッシュボード由来の変数が残っている可能性 (Team α 報告):

- `templates/tabs/overview.html`: `{{AVG_AGE}}=月給`, `{{MALE_COUNT}}=正社員数` (V1 求職者由来の意味のすり替え)
- `templates/tabs/balance.html`, `workstyle.html`, `demographics.html`: 同種チェック必須

確認コマンド:

```bash
grep -nE '\{\{(AVG_AGE|MALE_COUNT|FEMALE_COUNT|TOTAL_SEEKERS)\}\}' templates/tabs/
```

V1 残骸の場合、**ナビ復活してもデータ整合性破綻のため復活推奨せず**。完全削除へ進む。

---

## 4. Stage 3: 削除前依存チェーン確認 (memory `feedback_partial_commit_verify.md` 遵守)

削除候補ハンドラが他から参照されていないことを確認:

```bash
# ルート定義
grep -nE '/tab/(overview|balance|workstyle|demographics|trend|insight)' src/

# ハンドラ参照
grep -rn 'tab_overview\|tab_balance\|tab_workstyle\|tab_demographics\|tab_trend\|tab_insight' src/

# pub mod 宣言
grep -n 'pub mod \(overview\|balance\|workstyle\|demographics\|trend\|insight\)' src/handlers/mod.rs

# include_str! 経由のテンプレート参照
grep -rn 'include_str!.*tabs/\(overview\|balance\|workstyle\|demographics\)' src/

# E2E テスト
grep -rn '/tab/\(overview\|balance\|workstyle\|demographics\|trend\|insight\)' . --include='*.py'
```

判明した参照箇所すべての更新計画を立てた **後** に削除する。

---

## 5. Stage 4: 削除実行前チェックリスト

🔴 **本リストの全項目に check が入るまで削除コミット作成禁止**:

- [ ] Stage 1 のログ確認を完了し、判定 (A/B/C) を文書化した
- [ ] Stage 2 のテンプレ遺物チェックを完了した
- [ ] Stage 3 の依存チェーン全箇所を列挙した (`grep` 結果を保存)
- [ ] E2E テスト 4 ファイル (`e2e_8fixes_verify.py`, `e2e_chart_json_verify.py`, `e2e_c1_c4_coverage.py`, `docs/E2E_TEST_PLAN*.md`) の更新計画がある
- [ ] `cargo test --lib` がローカルで全パスする状態である (関連変更なし)
- [ ] 並行作業 (PDF 再構成 P1/P2, jobmap 修正親セッション) との衝突がない
- [ ] PR description に Stage 1 ログ判定結果と削除根拠を明記する準備ができた

---

## 6. 削除手順 (判定 C の場合のみ)

サブモジュール 1 件あたり 1 commit で実施:

```
Commit N: chore: remove dead route /tab/<name>
- src/lib.rs: route definition 削除
- src/handlers/<name>.rs: ファイル削除
- src/handlers/mod.rs: pub mod <name>; 削除
- templates/tabs/<name>.html: 削除
- e2e_*.py: /tab/<name> 行を削除/コメントアウト
```

🔴 `/api/insight/*` を維持する場合:
- `tab_insight` 関数のみ削除
- `pub fn render_insight_report_page` など API 用関数は **削除禁止**
- `src/handlers/insight/mod.rs` の `pub mod` 構造は維持

---

## 7. ロールバック手順

万一外部利用が判明した場合:

```bash
# 削除コミットを revert
git revert <delete_commit_sha>
git push

# Render に再デプロイ
# → 自動的にルート復活
```

git 履歴に残るため復元コストは低いが、`include_str!` 経由のテンプレート消失や E2E テスト変更の戻しが必要。

---

## 8. 参考

- `docs/audit_2026_04_24/plan_p3_code_health.md` #5 (本ドキュメント策定根拠)
- `docs/audit_2026_04_24/team_delta_codehealth.md` (dead route 監査詳細)
- `docs/contract_audit_2026_04_23.md` (insight API 公開状況)
- memory `feedback_partial_commit_verify.md` (依存チェーン確認義務)
- memory `feedback_git_safety.md` (削除作業時の安全運用)
