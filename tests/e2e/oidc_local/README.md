# Google OIDC ログインのローカル E2E (実ブラウザ)

本物の Google を使わずに、ログインの流れを Chromium で通す。偽 Google は `127.0.0.1:9300`、
アプリは `localhost:8080` で動かす。ホストが違うので Google からの戻りと同じクロスサイト遷移になり、
`RENDER=1` (本番と同じ Secure + SameSite=Strict のセッション Cookie) での挙動を確かめられる。

偽 Google はアカウント選択画面を出し、テストがそれをクリックして戻る。自動で戻すと戻りの
ナビゲーションの起点がアプリ側 (同一サイト) になり、Strict Cookie の問題を見逃す
(2026-09-29 に、成功時の応答を 303 に差し替えると失敗することをこの形で確認済み)。

依存: `pip install pyjwt cryptography playwright`

```bash
cargo build -j 2 --bin rust_dashboard            # debug ビルド (Discovery の上書きは debug のみ有効)
python tests/e2e/oidc_local/fake_google.py .      # 偽 Google (リポジトリのルートを渡す)

# OIDC 有効 (23 項目)
PORT=8080 AUTH_PASSWORD=testpass ALLOWED_DOMAINS=f-a-c.co.jp RENDER=1 HELLOWORK_DB_PATH=data/none.db \
GOOGLE_OIDC_CLIENT_ID=test-client GOOGLE_OIDC_CLIENT_SECRET=test-secret \
GOOGLE_OIDC_REDIRECT_URL=http://localhost:8080/auth/google/callback GOOGLE_OIDC_HOSTED_DOMAIN=f-a-c.co.jp \
GOOGLE_OIDC_DISCOVERY_URL_DEBUG=http://127.0.0.1:9300/.well-known/openid-configuration \
./target/debug/rust_dashboard &
python tests/e2e/oidc_local/e2e.py enabled <スクショ出力先>

# OIDC 未設定の回帰 (4 項目): GOOGLE_OIDC_* を外してアプリを起動し直す
python tests/e2e/oidc_local/e2e.py disabled <スクショ出力先>
```

ポート 8080 を使うのは、CSRF の許可 Origin (`src/lib.rs` ALLOWED_ORIGINS) に localhost:8080 が
入っていて、パスワードログインの POST が通るため。
