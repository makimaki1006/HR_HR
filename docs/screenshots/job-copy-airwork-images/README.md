# AirWork画像表示の確認

1440×742、架空求人・架空素材だけで撮影。実際の顧客名・口座・画像は含まない。1920×742でも同じE2Eを実行する。

- 01: 求人内容で5枚を表示
- 02: ポップアップで5枚を表示（縦方向へスクロールできる）
- 03: 掲載終了求人の取得不可理由

再生成: `JOB_COPY_AWIMG_LOCAL=1 npx playwright test -c tests/e2e/job-copy-airwork-images.config.ts`。
