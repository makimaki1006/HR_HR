# 本番監視チェックリスト (POST_RELEASE_MONITORING_CHECKLIST)

**最終更新**: 2026-05-06
**対象本番**: https://hr-hw.onrender.com
**直近反映 commit**: `5d0d86d`
**主要 spec**: `tests/e2e/market_intelligence_*.spec.ts`

本ドキュメントは V2 (HelloWork) 本番リリース後の継続監視手順をまとめる。Render 無料プラン特性 (cold start) と、MarketIntelligence variant ガード (Step 5 マーカーが Full / Public / default に混入していないこと) の確認を最優先とする。

---

## 1. 毎日確認

実施者: 当番 (1 人)。所要 5〜10 分。

- [ ] `/health` 200 応答
  - `curl -i https://hr-hw.onrender.com/health`
  - 200 以外なら Section 4 を参照し rollback 判断へ。
- [ ] Render deploy status を Render dashboard で確認
  - 最新 deploy が `Live` であること。
  - Build / Deploy log にエラー (panic / OOM / migration failure) がないこと。
- [ ] login success (手動 1 回)
  - 本番にログインし、トップ画面が描画されることを目視確認。
- [ ] MarketIntelligence variant 表示確認
  - default / Full / Public / Step 5 (resident) variant をそれぞれ 1 回ずつ開く。
  - 各 variant で想定セクションが描画されること。
- [ ] Full / Public / default に Step 5 マーカー混入なし
  - Full / Public / default レンダリング HTML を取得し以下が含まれないこと:
    - `mi-parent-ward-ranking`
    - その他 Step 5 専用セクション ID (variant guard 内のみ出現する想定のもの)
  - 検証コマンド例: `curl -s "<本番 URL>/<variant path>" | grep -c "mi-parent-ward-ranking"` が `0` であること。
  - Step 5 (resident) variant でのみ `1` 以上を許容。

---

## 2. 週次確認

実施者: 当番。所要 30 分。

- [ ] 本番向け E2E 実行
  - `BASE_URL=https://hr-hw.onrender.com npx playwright test market_intelligence`
  - 連続失敗時は Section 4 へ。
- [ ] 主要テーマの描画
  - `default` / `v8` / `v7a` テーマで MarketIntelligence を開き ECharts 初期化済みを目視確認 (canvas + chart 値)。
- [ ] print emulation 確認
  - Playwright `emulateMedia({ media: 'print' })` で主要セクションが残存し、本文幅が縮まないこと。
  - 既知の `@page` 重複定義による横幅縮みデグレが再発していないこと。
  - 印刷向けブロック (`mi-print-summary` / `mi-print-annotations`) が表示され、Hard NG 用語 (推定人数 / 想定人数 等) が紙面に出ていないこと (詳細: `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md` §7)。

---

## 3. 見るべき異常

下記いずれか観測時は即時 Section 4 の rollback 判断に進む、または当番リーダーへ連絡する。

- report render timeout
  - Render free tier の cold start 由来 (初回 60s 程度) は別扱いとし、2 回目以降のリクエストで継続するもののみを異常扱いする。
- Turso read error (libSQL connection error / read timeout)
- upload 403 (CSRF 許可 Origin 不整合 — Origin / Referer ヘッダ不一致)
- Hard NG 表現混入 (本番出力 HTML に下記用語が含まれていないこと):
  - `target_count`
  - `推定人数`
  - `想定人数`
  - `母集団人数`
  - これらは V2 出力では絶対 NG (規約)。検出ガイドライン用語として本ドキュメントに引用しているのみで、本番テンプレート / HTML / レポート文面には絶対に出力されてはならない。
- resident `estimated_beta` の人数表示 (絶対 NG)
  - resident variant において beta 値はそのまま「人数」として表示してはならない。比率 / 指数 / ランクのいずれかで提示すること。
- `parent_rank` 欠落 / `national_rank` との順序逆転
  - parent_rank が null / 欠落していないか。
  - national_rank ≦ parent_rank の関係 (※同一被参照セットでの順序前提) が逆転していないか。

---

## 4. rollback 判断基準

下記いずれかに該当する場合、当番は当番リーダーに連絡し rollback を提案する。

- 本番 E2E 連続 FAIL
  - ただし UI 実装起因 (テスト側のセレクタ更新漏れ等) と判定された場合のみ rollback 候補とし、テストデータ / Render 一時障害が原因と判定された場合は除外する。
- Full / Public / default variant への Step 5 マーカー混入
  - variant guard が崩れている兆候。即時 rollback 対象。
- resident variant での人数表示混入 (`estimated_beta` が人数として表示されている)
- 認証 / CSRF 広範囲障害
  - 一般ユーザーがログインまたは upload を成立できない状態。

rollback 操作自体はユーザー / 当番リーダーの明示的承認を経てから実施する。

---

## 5. 記録形式

毎日確認 / 週次確認 / 異常発生時は下記テーブルに 1 行追記する。`commit` は確認時点の本番反映 commit を記載。

| date | commit | /health | E2E | observed issue | action |
|---|---|---|---|---|---|
| 2026-05-06 | 5d0d86d | 200 | 10/10 PASS | none | release confirmed |

記入ルール:
- `E2E` 欄は毎日確認時は `n/a` 可。週次の `npx playwright test market_intelligence` 実行時は `<pass>/<total>` 形式で記録。
- `observed issue` は Section 3 の用語を使い、用語が無い場合は短い英語要約を残す。
- `action` は `release confirmed` / `monitor` / `rollback proposed` / `rollback executed` / `hotfix <commit>` のいずれかを選ぶ。
