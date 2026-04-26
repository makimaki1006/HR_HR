# Bug Marker テスト運用ルール

**作成日**: 2026-04-26
**作成者**: Refactoring Expert (Agent E3)
**目的**: `#[ignore]` 付き bug marker テストが「修正完了後も silently ignored のまま放置されるリスク」を再発防止する
**配置先 (本来)**: `docs/bug_marker_workflow.md` (sandbox 制約により audit ディレクトリに暫定配置)

---

## 1. 原則

| ルール | 内容 |
|---|---|
| 起票 | 既知の不具合を「再現する失敗テスト」として書き、`#[ignore = "<bug_id> <一行説明>"]` を付与 |
| 命名 | テスト名は `bug_marker_<scope>_<symptom>_bug_marker` で固定。grep 容易性を確保 |
| 修正 | バグ修正 commit と **同じ commit** で `#[ignore]` 行を削除し、テストを active 化する |
| ライフサイクル | `#[ignore]` 状態の bug marker が 24 時間を超えたら CI で warning。3 ヶ月超えたらレビュー対象 |

---

## 2. 対象テスト (2026-04-26 時点)

`team_delta_codehealth.md` および `plan_p3_code_health.md #12` で特定された現役 bug marker:

| テスト名 | 対象バグ | 親セッション |
|---|---|---|
| `bug_marker_seekers_marker_name_key_MISSING_bug_marker` | jobmap Mismatch #1 | 親セッション P0 担当 |
| `bug_marker_labor_flow_returns_municipality_key` | jobmap Mismatch #4 | 親セッション P0 担当 |

---

## 3. 修正 PR チェックリスト

修正 PR を作成する開発者は以下を必ず確認する:

- [ ] バグの根本原因を特定した (memory `feedback_never_guess_data.md` 遵守)
- [ ] 同 PR の commit で対象テストの `#[ignore]` 行を削除した
- [ ] ローカルで `cargo test --include-ignored` を実行し全パスを確認した
- [ ] PR description に `Closes bug_marker_<name>` を明記した
- [ ] CI 緑であることを確認した

---

## 4. CI 自動チェック

`scripts/check_ignored_tests.sh` を CI に組み込む。雛形は本ドキュメントの末尾に記載。

### 雛形 (`scripts/check_ignored_tests.sh`)

```bash
#!/usr/bin/env bash
# 用途: #[ignore] 付き bug marker テストを検出して件数を報告する
# 失敗化はせず警告のみ (段階的厳格化)

set -euo pipefail

# Rust source 配下の #[ignore] を grep
ignored=$(grep -rn '^\s*#\[ignore' src/ tests/ 2>/dev/null || true)
count=$(echo "${ignored}" | grep -c '^' 2>/dev/null || echo 0)

if [ "${count}" -gt 0 ]; then
  echo "::warning::ignored tests detected: ${count}"
  echo "${ignored}"
fi

# 段階 2 (将来): count > 0 なら exit 1 で CI red 化
# exit 0 (現状)
```

### GitHub Actions 統合例 (`.github/workflows/ci.yml` 追記)

```yaml
- name: Check ignored bug markers
  run: bash scripts/check_ignored_tests.sh
```

---

## 5. レビュアーガイドライン

PR レビュー時に以下を確認:

| 観点 | 確認内容 |
|---|---|
| 新規 `#[ignore]` 追加 | bug ID と一行説明が attribute コメントにあるか |
| 既存 `#[ignore]` 削除 | 同 PR で実コードの fix が含まれているか |
| テスト名 | `bug_marker_*` 命名規約に従っているか |
| ドキュメント | 本ファイルへの追記 (新 bug marker 起票時) があるか |

---

## 6. 段階的厳格化ロードマップ

| Phase | タイミング | 内容 |
|---|---|---|
| Phase 1 (現状) | 2026-04-26〜 | warning のみ。CI red 化なし |
| Phase 2 | 2026-05-15 以降 | 24 時間超 ignored は warning, 1 週間超は CI red |
| Phase 3 | 2026-06 以降 | 全 `#[ignore]` 件数を 0 で merge ブロック |

---

## 7. 参考

- memory `feedback_agent_contract_verification.md` (採用診断 8 panel 全滅事故): 並列 agent 間契約検証必須
- `docs/audit_2026_04_24/team_delta_codehealth.md` 2.5 節 (bug marker 現状)
- `docs/audit_2026_04_24/plan_p3_code_health.md` #12 (本ドキュメント策定根拠)
