# AirWork画像URL表示の確認

1440×742、架空求人・架空素材だけで撮影。実際の顧客名・口座・画像は含まない。

- 01: 求人内容でAirWorkの画像5枚を順番どおりに表示
- 02: ポップアップで5枚を表示
- 03: ポップアップの「この求人には画像がありません」
- 04: 求人内容の「この求人には画像がありません」

再生成: `JOB_COPY_AWIMG_LOCAL=1 npx playwright test -c tests/e2e/job-copy-airwork-image-urls.config.ts`。
