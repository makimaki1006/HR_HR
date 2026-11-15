import { beforeEach } from 'vitest';

// 本人の担当者は画面が読み込むたびに API で確かめる（selfOwner.ts）。テストでは既定で「分からない」にして呼ばない。
// 本人を決めたいテストは setSelfOwnerForTest('11') を呼ぶ。api client のモックより先に掴まないよう動的に読む。
beforeEach(async () => {
  const self = await import('../screens/job-copy/selfOwner');
  self.setSelfOwnerForTest(null);
});
