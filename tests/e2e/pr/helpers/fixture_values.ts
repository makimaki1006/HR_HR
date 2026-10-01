/**
 * scripts/e2e/make_fixture_db.py の postings fixture の既知値。
 * fixture の行を足したら、ここも更新する (smoke.spec.ts が /health の db_rows で照合する)。
 */
export const FIXTURE = {
  postingsTotal: 63,
  // 都道府県 → 市区町村 (JIS 北→南順で /api/prefectures は東京都、大阪府の順)
  prefectures: ['東京都', '大阪府'],
  municipalities: {
    東京都: ['千代田区', '新宿区', '港区'], // /api/municipalities_cascade は ORDER BY municipality (文字コード順)
    大阪府: ['堺市', '大阪市'],
  },
  // 千代田区: 正社員 10 / パート 5
  chiyoda: { fulltime: 10, parttime: 5, total: 15 },
} as const;

export const E2E_EMAIL = 'e2e@f-a-c.co.jp';
export const E2E_PASSWORD = 'testpass';
export const PR_PORT = 9217;
export const PR_BASE_URL = `http://localhost:${PR_PORT}`;
