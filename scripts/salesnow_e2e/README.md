# 企業データ地図 の E2E 検証ハーネス

本番 Turso の認証情報が無い環境で、実データを通した検証を行うための一式。
libSQL HTTP Pipeline API 互換のスタブを立て、実 CSV を積んだローカル SQLite を
バックエンドにして、実 Rust サーバを本番と同じ経路で動かす。

```
実 CSV (213,145 行) → ローカル SQLite → libSQL 互換スタブ → 実 Rust サーバ → ブラウザ/HTTP
```

## 使い方

```bash
# 1. CSV から SQLite を作る (数分)
python scripts/salesnow_e2e/e2e_build_db.py

# 2. libSQL 互換スタブを起動 (別シェル)
STUB_PORT=9401 python scripts/salesnow_e2e/e2e_turso_stub.py

# 3. サーバを起動 (SalesNow の向き先をスタブにする)
PORT=9311 AUTH_PASSWORD=... ALLOWED_DOMAINS=f-a-c.co.jp \
  SALESNOW_TURSO_URL=http://127.0.0.1:9401 SALESNOW_TURSO_TOKEN=stub \
  cargo run --bin rust_dashboard

# 4. 検証
python scripts/salesnow_e2e/e2e_crosscheck_all.py         # 全47都道府県×全業種を独立計算と突合
python scripts/salesnow_e2e/e2e_company_profile_check.py  # 企業プロフィールの抑制表示
python scripts/salesnow_e2e/e2e_probe_api.py              # ゲートの効き具合を地域別に測る
BASE_URL=http://127.0.0.1:9311 E2E_EMAIL=... E2E_PASS=... \
  npx playwright test tests/e2e/labor_flow_headcount_gate.spec.ts
```

## 注意

- `e2e_build_db.py` が読む CSV のパスは環境依存。実行前に確認すること
- スタブは検証専用。認証を一切見ないので、外部に公開しないこと
- 生成される `e2e_salesnow.db` は数百 MB になる。リポジトリに入れないこと
  (`scripts/salesnow_e2e/*.db` は .gitignore 済み)
