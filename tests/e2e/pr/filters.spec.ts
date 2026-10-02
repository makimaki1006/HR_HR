import { expect, test, type Page } from '@playwright/test';
import { FIXTURE } from './helpers/fixture_values';
import { login } from './helpers/login';

// ヘッダーフィルタ (旧シェルの #pref-select / #muni-select と、React AppShell filters の
// #hr-pref-select / #hr-muni-select) が fixture の都道府県・市区町村を同じ順で並べる。
// option HTML 取得 (/api/prefectures, /api/municipalities_cascade) は Accept: text/html で呼ぶ。

/** value が空 (「全国」「全市区町村」の見出し) を除いた option の value。 */
async function optionValues(page: Page, selector: string): Promise<string[]> {
  const values = await page.locator(`${selector} option`).evaluateAll((els) =>
    els.map((e) => (e as HTMLOptionElement).value),
  );
  return values.filter((v) => v !== '');
}

async function checkFilters(
  page: Page,
  prefSel: string,
  muniSel: string,
  staticPrefList: boolean,
): Promise<void> {
  // (c) 都道府県 (fixture は東京都・大阪府)。React は /api/prefectures (fixture の 2 件)、
  // 旧画面は templates の固定 47 件なので、fixture の 2 件を取り出して順序 (東京都, 大阪府) を確かめる。
  await expect
    .poll(async () => {
      const all = await optionValues(page, prefSel);
      return staticPrefList ? all.filter((v) => (FIXTURE.prefectures as readonly string[]).includes(v)) : all;
    })
    .toEqual([...FIXTURE.prefectures]);
  if (staticPrefList) expect(await optionValues(page, prefSel)).toHaveLength(47);
  // (a)/(b) 東京都を選ぶと市区町村が千代田区・新宿区・港区の順
  await page.selectOption(prefSel, '東京都');
  await expect.poll(() => optionValues(page, muniSel)).toEqual([...FIXTURE.municipalities.東京都]);
  // 大阪府へ切り替えると入れ替わる (前の都道府県の値が残らない)
  await page.selectOption(prefSel, '大阪府');
  await expect.poll(() => optionValues(page, muniSel)).toEqual([...FIXTURE.municipalities.大阪府]);
}

test.describe('ヘッダーフィルタ', () => {
  test('旧画面 (/) の都道府県・市区町村 select が fixture の値を並べる', async ({ page }) => {
    await login(page);
    await checkFilters(page, '#pref-select', '#muni-select', true);
  });

  test('React 画面 (/app/dummy) の都道府県・市区町村 select が fixture の値を並べる', async ({ page }) => {
    await login(page);
    await page.goto('/app/dummy');
    await expect(page.locator('#hr-pref-select')).toBeEnabled();
    await checkFilters(page, '#hr-pref-select', '#hr-muni-select', false);
  });

  test('option 取得は Accept: text/html で送られる', async ({ page }) => {
    await login(page);
    const accepts: string[] = [];
    page.on('request', (req) => {
      if (req.url().includes('/api/prefectures') || req.url().includes('/api/municipalities_cascade')) {
        accepts.push(req.headers()['accept'] ?? '');
      }
    });
    await page.goto('/app/dummy');
    await expect(page.locator('#hr-pref-select')).toBeEnabled();
    await expect.poll(() => optionValues(page, '#hr-pref-select')).toEqual([...FIXTURE.prefectures]);
    await page.selectOption('#hr-pref-select', '東京都');
    await expect.poll(() => optionValues(page, '#hr-muni-select')).toEqual([...FIXTURE.municipalities.東京都]);
    expect(accepts.length).toBeGreaterThanOrEqual(2);
    expect(accepts.every((a) => a === 'text/html')).toBe(true);
  });
});
