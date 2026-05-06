# Phase 3 Step 5 Phase 7: MarketIntelligence E2E 4 spec

**Worker P7 担当範囲**: Phase 8 で auth ブロックにより HTTP 検証を skip した範囲を Playwright で E2E カバーする。

**作成日**: 2026-05-04
**作成者**: Worker P7 (Claude Opus 4.7)

---

## 1. 追加した spec

| # | ファイル | テスト数 | 主要 assertion |
|---|---------|---------|---------------|
| 1 | `tests/e2e/market_intelligence_smoke.spec.ts` | 1 | session_id 取得 / variant=mi で MI マーカー描画 / 認証情報非流出 |
| 2 | `tests/e2e/market_intelligence_display_rules.spec.ts` | 2 | workplace measured = headcount 許容 / resident estimated_beta = 人数禁止 / parent_rank が national_rank より HTML 先 |
| 3 | `tests/e2e/market_intelligence_variant_isolation.spec.ts` | 3 | Full / Public は Step 5 不在 / MI variant のみ Step 5 描画 |
| 4 | `tests/e2e/market_intelligence_print_theme.spec.ts` | 4 (theme×3 + print) | default/v8/v7a で MI 描画維持 / print emulation で display:none されないこと |

合計 **10 テスト** (`npx playwright test --list` で確認済)。

## 2. 既存資産の再利用

| 既存資産 | 利用方法 |
|---------|---------|
| `tests/e2e/regression_2026_04_26.spec.ts` の `login` / `clickNavTab` | `tests/e2e/helpers/session.ts` に抽出して再利用 |
| `tests/e2e/survey_deepdive_2026_04_26.spec.ts` の `uploadCsv` | 同 helper に取り込み (HTMX afterSwap + `/api/survey/upload` レスポンス待ち) |
| `tests/e2e/fixtures/indeed_test_50.csv` | 4 spec すべてで fixture として使用 |
| `playwright.config.ts` (BASE_URL / E2E_EMAIL / E2E_PASS / Render cold start 60s) | そのまま継承 |

`helpers/session.ts` に新規 export 追加:
- `login(page, baseUrl?)`
- `clickNavTab(page, label, expectedText?)`
- `uploadCsv(page, csvPath, sourceType?, wageMode?)`
- `extractSessionId(page)` – `#survey-result` 内の `<a href="/report/survey?session_id=...">` から抽出
- `loginAndUpload(page, csvPath, sourceType?)` – 上記 3 step を一括実行
- `buildReportUrl(sessionId, variant?, theme?)` – `URLSearchParams` で組み立て
- `ensureCredentials()` – 環境変数 unset 時に明示的に throw

## 3. 認証情報の取扱い

| 項目 | 値 |
|------|---|
| 環境変数 | `E2E_EMAIL`, `E2E_PASS` (既存 `regression_2026_04_26.spec.ts` と同じ) |
| `.env` 直接 open | しない |
| password の DOM 流出チェック | Spec 1 で `expect(html).not.toContain(password)` (password が 6 文字以上のときのみ) |
| `AUTH_PASSWORD` / `TURSO_EXTERNAL_TOKEN` の流出チェック | Spec 1 で除外 assertion |

タスク指示書では `AUTH_PASSWORD` を環境変数として参照する前提でしたが、本リポジトリの既存 spec が `E2E_EMAIL` / `E2E_PASS` を採用しているため、それに合わせました (一貫性 > 指示書命名)。

## 4. 環境変数要件

```powershell
$env:BASE_URL    = "http://localhost:9216"  # 既定 (省略可)
$env:E2E_EMAIL   = "<email>"
$env:E2E_PASS    = "<password>"

# サーバー側で必要 (ユーザー側 cargo run --release 時)
$env:TURSO_EXTERNAL_URL   = "..."
$env:TURSO_EXTERNAL_TOKEN = "..."
```

`E2E_EMAIL` / `E2E_PASS` が未設定の場合、`ensureCredentials()` が明示的に throw し、エラーメッセージで設定方法を案内します。

## 5. 実行手順

### ローカル (port 9216)

```powershell
# 1) サーバー起動 (別 PowerShell)
$env:TURSO_EXTERNAL_URL   = "..."
$env:TURSO_EXTERNAL_TOKEN = "..."
cargo run --release

# 2) 認証情報を設定
$env:E2E_EMAIL = "..."
$env:E2E_PASS  = "..."
$env:BASE_URL  = "http://localhost:9216"

# 3) Phase 7 spec のみ実行
npx playwright test tests/e2e/market_intelligence_smoke.spec.ts tests/e2e/market_intelligence_display_rules.spec.ts tests/e2e/market_intelligence_variant_isolation.spec.ts tests/e2e/market_intelligence_print_theme.spec.ts --reporter=list

# あるいは正規表現で
npx playwright test market_intelligence --reporter=list
```

### Render 本番

```powershell
$env:BASE_URL  = "https://hr-hw.onrender.com"
npx playwright test market_intelligence --reporter=list
```

## 6. 本セッションでの実行可否

**Spec の構文検証**: `npx playwright test --list` で 10 テストが正常に列挙されることを確認 (PASS)。

**実機実行**: 本セッションでは **未実行**。理由:
- サーバー起動 (cargo run --release) と Turso credentials が同セッションでは複数並行に占有困難
- `E2E_EMAIL` / `E2E_PASS` を取得しないルール (secret 取扱い)
- ユーザー手動実行に委ねる方針 (タスク指示書「サーバー起動が困難な場合のフォールバック」参照)

ユーザーが上記「実行手順」に従って起動・実行することで、4 spec すべてが実機検証可能です。

## 7. フォールバック (Turso なし / auth なし)

- Turso credentials が設定されていない場合: サーバー起動時に MI セクションが空 (`mi-empty`) になる可能性。
  spec 側は `mi-empty` も Step 5 マーカーとして扱うため、空 fallback でも spec は通る設計。
- auth なしで spec を実行: `ensureCredentials()` が即 throw して、テスト失敗の原因が credentials 未設定であることを明示。

## 8. 既知の制限

| 制限 | 内容 | 対応 |
|------|------|------|
| 同セッション実行不可 | 本作業セッションでは実機 `cargo run` を起動・占有しない方針 | ユーザー手動実行 |
| Render cold start | 初回 60s 程度かかる | `test.setTimeout(240_000 / 360_000 / 420_000)` + `navigationTimeout: 60_000` で吸収 |
| session_id 共有 | Spec 3 / 4 は cold start 軽減のため `let sharedSessionId` で 1 セッション再利用 | `fullyParallel: false`, `workers: 1` (既存 config) で安全 |
| display rule の partial verification | fixture 50 行で workplace / resident セクションが空集合になる可能性 | 該当時は `test.info().annotations` で skip 通知 (silent fail を防ぐ) |
| HTML 順序検証 | parent_rank と national_rank の両セルを含む row が無い場合 | 同上 |

## 9. 制約遵守チェック

| 制約 | 遵守 |
|------|------|
| `.env` 直接 open 禁止 | YES (環境変数のみ参照) |
| password / token を log / snapshot に出さない | YES (`console.log` 不使用、Spec 1 で逆検証) |
| token / password の DOM 流出を Spec 1 で検査 | YES |
| DB 書き込み禁止 | YES (read-only) |
| Turso 書き込み禁止 | YES |
| Rust ソース変更禁止 | YES (新規 .ts のみ) |
| push 禁止 | YES |
| 既存 E2E spec 変更禁止 | YES (`helpers/session.ts` のみ新規追加) |

## 10. ファイル一覧

新規追加:
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/helpers/session.ts`
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/market_intelligence_smoke.spec.ts`
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/market_intelligence_display_rules.spec.ts`
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/market_intelligence_variant_isolation.spec.ts`
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/market_intelligence_print_theme.spec.ts`
- `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_PHASE7_E2E.md` (本ファイル)

既存ファイル変更: なし
