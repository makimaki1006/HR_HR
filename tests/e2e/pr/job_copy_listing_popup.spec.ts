import { test, expect } from '@playwright/test';
import { mkdir, readFile } from 'node:fs/promises';
import { login } from './helpers/login';
import { openOtherAnalysis, closeOtherAnalysis, findListings } from '../job-copy-navigation';
const screenshots = 'docs/screenshots/job-copy-listing-popup-v2';
const description = '出荷リストを確認し、日用品を棚から集めます。\n\n商品の種類と数量を照合し、配送先ごとに箱へまとめます。\n台車を使い、通路の安全を確認しながら出荷場所へ運びます。\n\n困ったときは近くの担当者へ確認してください。\n最後の作業では翌日の出荷予定を確認し、持ち場を片付けます。';
const awDescription = '届いた商品を配送先ごとに分ける仕事です。\n\n伝票を見て種類と数量を照合します。\n商品を台車へ載せ、指定された場所へ運びます。\n\nはじめは先輩と一緒に、伝票の見方と安全な運び方を覚えます。';
const summary = { occupation: '倉庫作業', location: '東京都江東区', salary_kind: '月給', salary_min: '213000', salary_max: '249000', hours: '9:00〜18:00', holidays: '土日休み', body_changed_at: '2026-10-11T00:00:00Z' };
const hrh = { id: '1', media: 'hrh', media_job_id: 'hidden-hrh-id', account_id: null, title: '架空の日用品ピッキングスタッフ', prefecture: '東京都', municipality: '江東区', category: null, publication_status: '非公開', last_csv_detected_at: null, application_count: 0, summary };
const aw = { ...hrh, id: '2', media: 'airwork', media_job_id: 'hidden-aw-id', title: '架空の出荷準備スタッフ', publication_status: '掲載中', application_count: null, summary: { ...summary, salary_min: null, salary_max: null, holidays: null } };
const fields: Record<string, string> = {
  求人id: 'hidden-job-id', 店舗id: 'hidden-shop-id', 職種id: 'hidden-occupation-id', 案件名: hrh.title,
  仕事内容: description, 給与形態: '月給', '基本給与 最小': '213000', '基本給与 最大': '249000',
  キャッチコピー: '日用品の出荷を、確認と安全な作業で支える仕事。', メリット: '日勤の仕事です。', 通勤経路: '架空中央駅から徒歩10分\n自転車での通勤も可能です。', 最寄り駅: '架空中央駅',
  雇用形態: '正社員', Indeed表示職種名: '倉庫内ピッキング', 応募資格: '資格・経験は問いません。',
  平均稼働時間: '8時間', 平均稼働日数: '20日', 固定残業代: 'なし', 想定残業時間: '月10時間',
  '試用・研修の有無': '1', '試用・研修時の雇用条件': '同条件', '試用・研修期の雇用形態': '正社員', '試用・研修期の給与のタイプ': '月給', '試用・研修期の基本給与 最小': '213000', '試用・研修期の基本給与 最大': '249000', '試用・研修期の平均稼働時間': '8時間', '試用・研修期の平均稼働日数': '20日', '試用・研修期の固定残業代': 'なし', '試用・研修期の想定残業時間': '月10時間', '試用・研修の詳細情報': '先輩と作業を進めます。',
  給与補足: '交通費は別途支給します。', 勤務時間: '9:00〜18:00（休憩60分）', 勤務時間帯: '日勤',
  自由項目1のタイトル: '休日・休暇', 自由項目1の内容: '土日休み\n年末年始休暇', 自由項目2のタイトル: '福利厚生', 自由項目2の内容: '社会保険・交通費支給',
  受動喫煙対策: '屋内禁煙', 受動喫煙についての補足情報: '屋外に喫煙所あり', 応募方法: '応募フォームで受け付けます。', 応募後のプロセス: '担当者が面談日程をご案内します。', 採用予定人数: '2名', 問い合わせ電話番号: '03-0000-0000', 連絡先メールアドレス: 'synthetic@example.invalid', 制作メモ: '担当者が文章を確認しています。', 公開: '', 公開開始日時: '2026-10-01 09:00',
};
for (let i = 1; i <= 4; i++) {
  fields[`仕事情報補足${String(i)}のタイトル`] = ['1日の進め方', '職場の案内', '入社後の流れ', '応募後の案内'][i - 1]!;
  fields[`仕事情報補足${String(i)}の内容`] = ['出荷予定を共有してから作業します。\n終了時は進捗を報告します。', '検品担当と出荷担当が連携する職場です。', '安全説明を受け、伝票の見方を練習します。', '担当者が勤務条件と入社日の希望を伺います。'][i - 1]!;
}
for (const viewport of [{ width: 1440, height: 742 }, { width: 1920, height: 1080 }]) test(`求人内容の全文・選択時点・閉じ方・一覧からの表示 ${viewport.width}`, async ({ page }) => {
  await page.setViewportSize(viewport);
  await login(page);
  await mkdir(screenshots, { recursive: true });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const columns = JSON.parse(await readFile('frontend/src/generated/jobgen/columns.json', 'utf8')) as string[];
  const body = columns.map(key => `${key}：${fields[key] ?? ''}`).join('\n');
  const oldBody = body.replace(description, '以前は食品の検品を担当していました。\n以前の本文の最後の文章です。');
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [hrh, aw], total: 2, offset: 0, next_offset: null, titles: [], deals: [{ id: '901', name: '架空物流の契約', count: 2 }], no_deal_count: 0, index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (url.pathname.endsWith('/listing-body-dates')) return route.fulfill({ json: { dates: { '1': summary.body_changed_at, '2': summary.body_changed_at } } });
    if (url.pathname.endsWith('/versions')) {
      const isAw = url.pathname.includes('/2/');
      return route.fulfill({ json: { listing: isAw ? { ...aw, application_count: null } : { ...hrh, application_count: null }, current_fields: isAw ? { 案件名: aw.title, 仕事内容: awDescription, 勤務地: '東京都江東区', 勤務時間: '9:00〜18:00', 給与形態: '月給', 雇用形態: '正社員', 福利厚生: '社会保険・交通費支給' } : { ...fields, 勤務地: '東京都江東区', '休日・休暇': '土日休み\n年末年始休暇', 福利厚生: '社会保険・交通費支給' }, versions: [
        { written_at: '2026-10-01T00:00:00Z', body: isAw ? '以前は商品の受け取りを担当していました。' : oldBody, image_urls: null },
        { written_at: '2026-10-11T00:00:00Z', body: isAw ? awDescription : body, image_urls: isAw ? null : ['/static/app/popup-fixture.svg'] },
      ], current: null, current_images: isAw ? { observed_at: '2026-10-11T01:00:00Z', image_urls: ['/static/app/popup-fixture.svg'] } : null, history_counts: isAw ? { shigotonaiyou: 2 } : { hrh_kyuujinhyou_honbun: 2 }, history_may_be_incomplete: false, can_write_drafts: false, drafts: [] } });
    }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.route('**/static/app/popup-fixture.svg', route => route.fulfill({ contentType: 'image/svg+xml', body: '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="240"><rect width="640" height="240" fill="#eaf5f7"/><text x="210" y="130" font-size="25" fill="#25394c">架空の職場の案内画像</text></svg>' }));
  await page.goto('/app/job-copy?listing=1');
  await findListings(page);
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  const opener = page.getByRole('button', { name: '求人の内容を確認する' });
  await expect(opener).toBeVisible();
  const bounds = (await opener.boundingBox())!;
  expect(bounds.y).toBeGreaterThanOrEqual(0); expect(bounds.y + bounds.height).toBeLessThan(viewport.height);
  expect(await page.locator('.jc-main').evaluate(element => element.scrollTop)).toBe(0);
  if (viewport.width === 1440) await page.screenshot({ path: `${screenshots}/01-detail-top-1440.png` });
  await opener.click();
  const dialog = page.getByRole('dialog', { name: hrh.title });
  await expect(dialog).toBeFocused();
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  await expect(dialog).toContainText('2026/10/11 09:00 JST');
  await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('月給 213,000円〜249,000円');
  await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('0件');
  await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('掲載終了');
  await expect(dialog.locator('.jc-listing-popup-missing').first()).not.toContainText(/公開状況|求人名|勤務地|給与形態/);
  expect(await dialog.locator('.jc-listing-popup-section').first().locator('p').textContent()).toBe(description);
  await expect(dialog).toContainText('確認が必要');
  await expect(dialog).not.toContainText(/hidden-job-id|hidden-shop-id|hidden-hrh-id|基本給与 最小|求人id|店舗id|shigotonaiyou/);
  const dialogBounds = (await dialog.boundingBox())!;
  expect(dialogBounds.x).toBe(viewport.width * 0.025);
  expect(dialogBounds.y).toBeGreaterThan(0);
  expect(await dialog.locator('.jc-listing-popup-scroll').evaluate(element => element.scrollWidth - element.clientWidth)).toBe(0);
  expect(dialogBounds.width).toBe(viewport.width * 0.95); expect(Math.abs(dialogBounds.height / viewport.height - 0.9)).toBeLessThan(0.01);
  expect(await page.evaluate(() => document.body.style.overflow)).toBe('hidden');
  await page.screenshot({ path: `${screenshots}/02-hrh-popup-${viewport.width}.png` });
  const scroller = dialog.locator('.jc-listing-popup-scroll');
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.locator('summary').first()).toBeInViewport();
  await expect(dialog.getByRole('img', { name: '画像1', exact: true })).toHaveCount(1);
  await page.screenshot({ path: `${screenshots}/04-missing-bottom-${viewport.width}.png` });
  const background = await page.locator('.jc-main').evaluate(element => element.scrollTop);
  await page.mouse.move(20, viewport.height / 2); await page.mouse.wheel(0, 500);
  expect(await page.locator('.jc-main').evaluate(element => element.scrollTop)).toBe(background);
  await page.keyboard.press('Tab');
  const focusInside = await dialog.evaluate(element => element.contains(document.activeElement)); expect(focusInside).toBe(true);
  await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0); await expect(opener).toBeFocused();
  await openOtherAnalysis(page, '本文・画像');
  await page.getByRole('button', { name: /2026\/10\/01 09:00 JST にHubSpotに記録された本文/ }).click();
  await closeOtherAnalysis(page);
  await opener.click(); await expect(dialog).toContainText('以前の本文の最後の文章です。'); await expect(dialog).not.toContainText('最後の作業では翌日の出荷予定を確認');
  await dialog.getByRole('button', { name: '求人の内容を閉じる' }).click(); await expect(opener).toBeFocused();
  await opener.click(); await page.mouse.click(20, 20); await expect(dialog).toHaveCount(0); await expect(opener).toBeFocused();
  await panel.getByRole('button', { name: `${aw.title}の本文を見る` }).click(); await opener.click();
  const awDialog = page.getByRole('dialog', { name: aw.title });
  expect(await awDialog.locator('.jc-listing-popup-section').first().locator('p').textContent()).toBe(awDescription);
  await expect(awDialog.locator('.jc-listing-popup-summary')).toContainText('月給 金額は未取得');
  await expect(awDialog.locator('.jc-listing-popup-summary')).toContainText('未取得');
  await expect(awDialog).toContainText('社会保険・交通費支給'); await expect(awDialog).toContainText('現在の取得値');
  if (viewport.width === 1440) await page.screenshot({ path: `${screenshots}/03-airwork-popup-1440.png` });
  await page.keyboard.press('Escape');
  await panel.getByText('絞り込み・並び順', { exact: true }).click(); await panel.getByLabel('取引', { exact: true }).selectOption('901');
  await panel.getByRole('button', { name: 'この取引の求人', exact: true }).click();
  const table = page.getByRole('table', { name: '取引の求人一覧表' });
  for (const [index, job] of [hrh, aw].entries()) {
    const rowOpener = table.locator('tbody tr').nth(index).getByRole('button', { name: '内容を見る', exact: true });
    await rowOpener.click();
    const rowDialog = page.getByRole('dialog', { name: job.title });
    await expect(rowDialog).toBeFocused(); await expect(rowDialog).toContainText(index === 0 ? description : awDescription);
    if (index === 0) await expect(rowDialog.locator('.jc-listing-popup-summary')).toContainText('0件');
    if (index === 0) {
      await page.keyboard.press('ArrowRight');
      await expect(page.getByRole('dialog', { name: aw.title })).toContainText(awDescription);
      await page.keyboard.press('ArrowLeft');
      await expect(page.getByRole('dialog', { name: hrh.title })).toContainText(description);
    }
    await page.keyboard.press('Escape'); await expect(rowOpener).toBeFocused(); await expect(table).toBeVisible();
  }
  await table.getByRole('button', { name: hrh.title, exact: true }).click(); await expect(opener).toBeVisible();
  await opener.click(); await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('0件');
  await page.keyboard.press('Escape');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.goto('/app/job-copy?demo=1');
  await page.locator('.jc-main').evaluate(element => { element.scrollTop = element.scrollHeight; });
  expect(await page.locator('.jc-main').evaluate(element => element.scrollTop)).toBeGreaterThan(0);
  await page.locator('.jc-job').filter({ hasText: '倉庫内ピッキングスタッフ' }).click();
  expect(await page.locator('.jc-main').evaluate(element => element.scrollTop)).toBe(0);
  const switched = (await opener.boundingBox())!;
  expect(switched.y).toBeGreaterThanOrEqual(0); expect(switched.y + switched.height).toBeLessThan(viewport.height);
});

for (const viewport of [{ width: 1440, height: 742 }, { width: 1920, height: 1080 }]) test(`初回50件の外の求人を直接開いて応募0件を表示 ${viewport.width}`, async ({ page }) => {
  await page.setViewportSize(viewport);
  await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const rows = Array.from({ length: 50 }, (_, index) => ({ ...hrh, id: String(index + 2), title: `架空の一覧求人${String(index + 2)}`, application_count: 99 }));
  const columns = JSON.parse(await readFile('frontend/src/generated/jobgen/columns.json', 'utf8')) as string[];
  const body = columns.map(key => `${key}：${fields[key] ?? ''}`).join('\n');
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: rows, total: 51, offset: 0, next_offset: 50, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/1/versions')) return route.fulfill({ json: { listing: hrh, versions: [{ written_at: '2026-10-11T00:00:00Z', body, image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?listing=1');
  await findListings(page);
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(panel.locator('.jc-hubspot-job')).toHaveCount(50);
  await expect(panel.getByRole('button', { name: `${hrh.title}の本文を見る` })).toHaveCount(0);
  await expect(panel).toContainText('応募 99件');
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog', { name: hrh.title });
  await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('0件');
  await expect(dialog.locator('.jc-listing-popup-summary')).not.toContainText('99件');
  await expect(dialog.locator('.jc-listing-popup-summary')).toContainText('掲載終了');
  await expect(dialog.locator('.jc-listing-popup-missing').first()).not.toContainText('公開状況');
  await expect(dialog).not.toContainText(/hidden-hrh-id|hidden-job-id|hidden-shop-id/);
  expect(await dialog.locator('.jc-listing-popup-scroll').evaluate(element => element.scrollWidth - element.clientWidth)).toBe(0);
  await mkdir(screenshots, { recursive: true });
  await page.screenshot({ path: `${screenshots}/05-outside-first-page-${viewport.width}.png` });
});

for (const viewport of [{ width: 1440, height: 742 }, { width: 1920, height: 1080 }]) test(`集計準備完了後に初回50件の外の応募0件を補う ${viewport.width}`, async ({ page }) => {
  await page.setViewportSize(viewport);
  await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  let ready = false;
  let bodyReads = 0;
  let countReads = 0;
  const rows = Array.from({ length: 50 }, (_, index) => ({ ...hrh, id: String(index + 2), title: `架空の一覧求人${String(index + 2)}`, application_count: 99 }));
  const columns = JSON.parse(await readFile('frontend/src/generated/jobgen/columns.json', 'utf8')) as string[];
  const body = columns.map(key => `${key}：${fields[key] ?? ''}`).join('\n');
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: ready ? 'ready' : 'preparing', listings: ready ? rows : [], total: ready ? 51 : null, offset: 0, next_offset: ready ? 50 : null, titles: [], index_built_at: ready ? '2026-10-11T00:00:00Z' : null, refreshing: !ready, refresh_failed: false } });
    if (path.endsWith('/1/versions')) {
      bodyReads++;
      return route.fulfill({ json: { listing: { ...hrh, application_count: null }, versions: [{ written_at: '2026-10-11T00:00:00Z', body, image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } });
    }
    if (path.endsWith('/1/applications')) { countReads++; return route.fulfill({ json: { listing_id: '1', application_count: ready ? 0 : null } }); }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?listing=1');
  await findListings(page);
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(panel.getByText('求人の一覧を準備しています', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog', { name: hrh.title });
  const applications = dialog.locator('.jc-listing-popup-summary > div').filter({ has: page.locator('dt', { hasText: '応募数' }) }).locator('dd');
  await expect(applications).toHaveText('未取得');
  expect(countReads).toBe(0);
  // Keep the popup open; the existing five-second list poll observes completion.
  ready = true;
  await expect(applications).toHaveText('0件');
  await expect(dialog).toBeFocused();
  await expect(panel.locator('.jc-hubspot-job')).toHaveCount(50);
  await expect(panel.getByRole('button', { name: `${hrh.title}の本文を見る` })).toHaveCount(0);
  expect(bodyReads).toBe(1); expect(countReads).toBe(1);
  await expect(dialog.locator('.jc-listing-popup-missing').first()).not.toContainText('公開状況');
  await expect(dialog).not.toContainText(/hidden-hrh-id|hidden-job-id|hidden-shop-id/);
  expect(await dialog.locator('.jc-listing-popup-scroll').evaluate(element => element.scrollWidth - element.clientWidth)).toBe(0);
  await mkdir(screenshots, { recursive: true });
  await page.screenshot({ path: `${screenshots}/06-cold-index-outside-first-page-${viewport.width}.png` });
});
