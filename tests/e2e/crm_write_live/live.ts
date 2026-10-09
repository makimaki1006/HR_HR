/** 実サーバ E2E の共通値 (すべて架空)。global-setup.ts と spec が共有する */
export const LIVE = {
  appPort: 9422,
  hubspotPort: 9420,
  tursoPort: 9421,
  app: 'http://localhost:9422',
  hubspot: 'http://127.0.0.1:9420',
  turso: 'http://127.0.0.1:9421',
  /** 偽 HubSpot の案件 0 (書き込み許可リストにある) と 案件 1 (許可リスト外) */
  allowedDealId: '9000000000',
  blockedDealId: '9000000001',
  /** 案件の担当者 (偽 HubSpot の owners[0]) のメール。非管理者としてログインする */
  userEmail: 'lt-user000@f-a-c.co.jp',
} as const;
