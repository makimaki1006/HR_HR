# 求人比較の表示確認

求人・顧客・画像・数値はすべて架空の固定応答です。実際の顧客情報は含みません。

- [同じ顧客](same-customer.png): 同じ案件を先頭に、20件ずつ相手を探す。
- [ほかの顧客](other-customers.png): 本文の職種・都道府県を引き継ぎ、解除・媒体・求人名で絞る。
- [3列の成果](three-columns.png): 掲載開始から28日、端にかかる週、応募・表示・クリック・費用の比較。
- [3列の本文・画像](three-columns-content.png): 違う項目の印と、同じ項目を開いた状態。

再撮影（先に `frontend` で `npm ci && npm run build`、サーババイナリを用意）:

```sh
E2E_BIN=/path/to/rust_dashboard npx playwright test \
  -c tests/e2e/pr/playwright.pr.config.ts job_copy_comparison.spec.ts --retries=0
```

画像は `test-results-pr` に出力されます。本文・画像の比較は最後に取得した内容で、集計期間当時の内容を保証するものではありません。
