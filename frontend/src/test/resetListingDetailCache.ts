import { afterEach } from 'vitest';

// 静的に import すると、テスト側の vi.mock('../../api/client') より先に本物の API 窓口を掴んでしまう。
// テストが読み込んだあとのモジュールを捨てるため、終了時に動的に読む。
afterEach(async () => {
  const cache = await import('../screens/job-copy/listingDetailCache');
  cache.resetListingDetailCache();
});
