# E2E 運用メモ (CSRF / ポート選定)

ローカル E2E 実行時に CSRF 起因の 403 で全 spec が落ちる事故を防ぐための運用メモ。

---

## 1. CSRF 許可 Origin (current)

`src/lib.rs` の `ALLOWED_ORIGINS` (定数) に列挙されている値が真正値。
2026-05-06 時点の実装値:

- `https://hr-hw.onrender.com`
- `http://localhost:3000`
- `http://localhost:8080`
- `http://127.0.0.1:3000`
- `http://127.0.0.1:8080`

書き込み系 (POST/PUT/PATCH/DELETE) のリクエストは `Origin` / `Referer` ヘッダーが
このリストに完全一致しない場合 403 で拒否される。許可リストを変更したい場合は
`src/lib.rs` の `ALLOWED_ORIGINS` を更新する (このメモも合わせて更新)。

---

## 2. 推奨ローカル E2E ポート

| ポート | 推奨度 | 備考 |
|--------|--------|------|
| 3000   | 推奨   | CSRF 許可済み。最も安全 |
| 8080   | 可     | CSRF 許可済みだが、既存 `rust_dashboard.exe` (V1) と衝突する可能性あり |
| 9216   | 注意   | `playwright.config.ts` の default URL になっているケースあり。CSRF 許可外の可能性が高いので、使用前に必ず `ALLOWED_ORIGINS` を確認 |
| 9316   | 使用禁止 | CSRF 許可外。upload POST が 403 で確定 FAIL する (2026-05-06 事故) |

事故の経緯: 2026-05-06 のローカル E2E で port 9316 を使った 1 回目は upload POST が 403 で全 spec FAIL。
2 回目に port 3000 へ切り替えたところ 10/10 PASS した。

---

## 3. 起動 + 認証手順テンプレート

PowerShell から以下のように起動する (値はローカルで設定する。docs に実値を書かないこと):

```
$env:PORT      = "3000"
$env:BASE_URL  = "http://localhost:3000"
$env:CARGO_TARGET_DIR = "target-e2e"
$env:E2E_EMAIL = "..."  # 実値はここに記載しない
$env:E2E_PASS  = "..."  # 実値はここに記載しない
cargo run --release
```

`CARGO_TARGET_DIR` を分けることで、エディタ側 `cargo check` のロックと衝突しない。
`E2E_EMAIL` / `E2E_PASS` は環境変数として渡すのみで、ファイルやログには絶対に残さない。

---

## 4. 403 が出たときのトリアージ

UI 実装の問題と判定する前に、必ず以下の 2 段階を順に通る:

1. **CSRF 許可 Origin チェック**
   - `BASE_URL` (例: `http://localhost:3000`) が `src/lib.rs` の `ALLOWED_ORIGINS` に
     完全一致で含まれているか確認する。
   - 含まれていなければ、許可済みポートに切り替えるのが先決 (UI コード変更しない)。
2. **CSRF token / cookie の問題を疑う**
   - 1 をクリアした上で 403 が出る場合、ログイン後の cookie / セッションや、
     Origin/Referer ヘッダーの送出有無を確認する。

この 2 段階を飛ばして UI/handler 実装に手を入れると、原因不明の修正が積み上がるので禁止。

---

## 5. 連続 login fail と rate limit

- 認証失敗が連続するとレート制限が発火する仕様。失敗 1 回で停止し、
  「認証情報が誤っているのか」「rate limit に既に入っているのか」を切り分けてから再試行する。
- 一度 rate limit に入った場合は cooldown を 10 分以上待つ。
  短い間隔で再試行すると cooldown がリセット相当に伸びる可能性があるため避ける。

---

## 関連

- 実装: `src/lib.rs` `ALLOWED_ORIGINS` / `check_csrf`
- E2E 設定: `playwright.config.ts`
- 過去事例: `docs/MANUAL_E2E_2026-04-26.md`, `docs/E2E_RESULTS_LATEST.md`
