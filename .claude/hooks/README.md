# .claude/hooks/

**hellowork-deploy プロジェクト限定** の Claude Code hook 一式。
Claude の暴走パターン (字面解釈の矮小化、検証省略、虚偽報告) を機械的に検出して block / warn する。

## スコープ限定

- 各スクリプト冒頭の `is_in_project(payload)` で **`Cargo.toml` + `src/handlers/survey/upload.rs` がある cwd でのみ発火**。
- 別プロジェクトで動かしても no-op (exit 0) になる。
- `.claude/settings.json` をリポジトリ直下に置くため、自動的にプロジェクト固定。

## Hook 一覧

| ファイル | 種別 | 発火条件 | 動作 |
|---|---|---|---|
| `check_completion_claim.py` | Stop | 「✅完了」「機能完了」「品質完了」「ALL PASS」「実装完了」等の強い完了主張 + 直前 30 ターンに `cargo test` / `pytest` / `curl` 等の検証 Bash 実行なし | **block** (exit 2) |
| `check_verification_claim.py` | Stop | 「整合性確認」「重複なし」「問題ない」「安全です」等の強い検証主張 + 直前 30 ターンに Bash / Read / Grep / Glob 使用なし | **block** |
| `check_pre_push.py` | PreToolUse(Bash) | `git push` コマンド + 直前 50 ターンに `test result: ok` 等のテスト成功ログなし | **block** |
| `check_db_write.py` | PreToolUse(Edit / Write / MultiEdit) | `*.db` `*.gz` `*.sqlite` `*.tar` `*.zip` への書き込み | **block** (ユーザー明示許可必要) |
| `inject_reverse_proof_context.py` | UserPromptSubmit | ユーザー発言に「逆証明 / 反証 / 反例 / 横展開 / 同種パターン / 不変条件 / invariant」 | **block しない**: コンテキスト注入で逆証明モードを通知 |
| `check_reverse_proof_response.py` | Stop | 直前 5 ユーザー発言に「逆証明」等あり、Claude 応答に「問題ない / OK / 全パス」等の弱い断言、かつ直近 30 ターンに Grep / Glob / grep 横断検査なし | **block** |
| `check_failure_patterns.py` | Stop | CLAUDE.md 重大事故記録 (2026-01-04 ～ 2026-03-10) と類似言い回し検出 | **warn のみ** (exit 0、stderr に注意) |
| `check_numeric_review_skill_used.py` | Stop | 数値/視覚レビュー完了主張 (例: 「視覚レビュー完了」「12/12 完了」) + 文脈が数値関連 + `.claude/.audit_numeric_done` marker が 60 分以内に touch されていない | **block** (`audit-numeric-anomaly` skill 呼出を要求) |

## Bypass

ユーザーが直近 5 発言で以下のいずれかを明言した場合、全 hook が bypass される:

| 表現 |
|---|
| 「テスト不要」「テスト無しで」「テストなしで」「テストスキップ」 |
| 「強制 push」「force push」 |
| 「hooks off」「フック停止」「フック無視」「hook 無効」 |

(`_lib.py::BYPASS_RE` で定義)

## 完全無効化

`.claude/settings.local.json` (gitignore 推奨) で個別 hook を上書き or 全 hook 無効化可能。

例: 全 hook 一時無効化:

```json
{
  "hooks": {}
}
```

## デバッグ

各 hook を手動でテスト可能。標準入力に hook イベントの JSON を渡す。

```bash
# UserPromptSubmit テスト
echo '{"cwd": "C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy", "transcript_path": "/dev/null", "prompt": "逆証明して"}' | python .claude/hooks/inject_reverse_proof_context.py

# is_in_project でスコープ外動作 (別ディレクトリ) になる事を確認
echo '{"cwd": "C:/tmp", "prompt": "逆証明して"}' | python .claude/hooks/inject_reverse_proof_context.py
# → 何も出力されない (no-op)
```

## 実装上の注意

- 標準ライブラリのみ使用 (jq / pip install 不要)。
- `_lib.py` に共通処理を集約。
- transcript jsonl の format (Claude Code の保存形式) を解析。
  - `type` または `role` が `user` / `assistant` の message を区別。
  - `content` が list の場合は `tool_use` / `tool_result` / `text` の dict を含む。
- 全 hook で例外発生時は exit 1 になり、Claude にエラー通知される (block にはならないが debug 必要)。

## 設計思想

- **私 (Claude) の自己申告に依存しない**: hook は Claude の応答を経由せず Claude Code runtime から直接実行される。
- **タスク種類非依存**: 検査対象は応答テキストのパターン + tool 使用履歴の有無。CSV / DB / UI / ドキュメント問わず効く。
- **false positive を避ける**: 強い言葉のみ (「✅完了」など最終報告に使う表現) に絞り、「修正しました」等の事実報告は対象外。
- **block 時は具体的な reason**: Claude が次にすべき行動 (テスト実行 / 横展開検査) が分かる形で stderr に出す。

## 過去経緯

2026-05-01 策定。Claude が以下のパターンを繰り返し起こしたため:

- 「✅完了」と書きながら基盤実装のみで動作未確認 (CLAUDE.md 完了定義違反)
- 「重複なし確認済み」と虚偽報告 (2026-01-05、追加課金)
- 「逆証明して」と再三言われたのに表面検査で済ませる
- ユーザー意図 (実 CSV 分析の新デザイン適用) を字面の最も簡単な解釈 (静的 HTML 配置) に矮小化

ルールベース (CLAUDE.md / memory) では効かなかったため、機械的検査に切替。
